//! Native audio decode: the first decodable audio track of the clip's container (MP4/MOV,
//! Matroska/WebM, MPEG-TS/PS), decoded with FilmCraft's AAC-LC, Opus and AC-3 / E-AC-3 decoders.
//!
//! Times count from the clip's first shown video frame, like [`crate::FrameReader`]'s, so a block
//! at `time` plays with the frame at `time`.

use crate::VideoError;
use crate::audio::{AudioBlock, AudioInfo, MAX_CHANNELS};
use crate::demux::{self, Clip, Container};
use crate::native::guarded;

/// Packets decoded (and dropped) before the first needed one: AAC and AC-3 overlap their MDCT
/// windows with the previous frame; Opus (CELT) needs a few for its pre-roll.
const PREROLL: usize = 3;
/// Timestamps further than 1/RESYNC_DIV s from the running sample count move it (gaps, drops).
const RESYNC_DIV: u32 = 50;
/// Consecutive undecodable packets before a read gives up.
const MAX_BAD_PACKETS: usize = 32;
/// Samples per channel one packet may decode to (E-AC-3 has 1536, Opus at most 5760).
const MAX_PACKET_SAMPLES: usize = 8192;

enum Track {
    Mp4 { track: usize },
    Mkv { track: usize },
    Ts { stream: usize },
}

enum Dec {
    /// ADTS framing (MPEG-TS) is stripped before decoding.
    Aac {
        dec: filmcraft_aac::Decoder,
        adts: bool,
    },
    Opus(filmcraft_opus::Decoder),
    Ac3(Box<filmcraft_ac3::Decoder>),
}

impl Dec {
    fn reset(&mut self) {
        match self {
            Dec::Aac { dec, .. } => dec.reset(),
            Dec::Opus(d) => d.reset(),
            Dec::Ac3(d) => d.reset(),
        }
    }

    /// Planar samples of one packet; `Ok(None)` for a packet that yields nothing (a dependent
    /// E-AC-3 substream).
    fn decode(&mut self, data: &[u8]) -> Result<Option<Vec<Vec<f32>>>, String> {
        match self {
            Dec::Aac { dec, adts } => {
                let au = if *adts {
                    let h = filmcraft_aac::AdtsHeader::parse(data).map_err(|e| e.to_string())?;
                    let end = h.frame_length.min(data.len());
                    data.get(h.header_length..end).ok_or("short ADTS frame")?
                } else {
                    data
                };
                dec.decode(au).map(Some).map_err(|e| e.to_string())
            }
            Dec::Opus(d) => d.decode(Some(data)).map(Some).map_err(|e| e.to_string()),
            Dec::Ac3(d) => {
                let out = d.decode(data).map_err(|e| e.to_string())?;
                Ok((!out.channels.is_empty()).then_some(out.channels))
            }
        }
    }
}

pub struct Reader {
    clip: Clip,
    track: Track,
    dec: Dec,
    info: AudioInfo,
    /// Start time of each packet in seconds from the video origin; None where the container has
    /// none (TS audio frames between PES timestamps).
    times: Vec<Option<f64>>,
    /// Next packet to decode.
    next: usize,
    /// Presentation time (samples at the source rate from the origin) of the next decoded sample.
    cursor: i64,
    /// Samples before this one are dropped (the target of the last seek).
    from: i64,
}

fn unsupported(why: impl Into<String>) -> VideoError {
    VideoError::Unsupported(why.into())
}

impl Reader {
    pub fn open(path: &std::path::Path) -> Result<Self, VideoError> {
        let clip = guarded("demux", || demux::open(path))?;
        guarded("audio", || Self::from_clip(clip))
    }

    fn from_clip(clip: Clip) -> Result<Self, VideoError> {
        let (track, dec, codec, times, rate, channels) = match &clip.container {
            Container::Mp4 { file, track: vt } => {
                use filmcraft_isobmff::{CodecConfig, TrackKind};
                let origin = file.tracks.get(*vt).and_then(|t| {
                    let tb = f64::from(t.timescale.max(1));
                    (0..t.samples.len()).filter_map(|i| t.presentation_pts(i)).min().map(|p| p as f64 / tb)
                });
                let mut found = None;
                for (i, t) in file.tracks.iter().enumerate() {
                    if t.kind != TrackKind::Audio || t.samples.is_empty() || t.timescale == 0 {
                        continue;
                    }
                    let a = t.audio().cloned().unwrap_or_default();
                    let dec = match t.codec() {
                        Some(CodecConfig::Aac(c)) => filmcraft_aac::Decoder::new(&c.asc).ok().map(|dec| (Dec::Aac { dec, adts: false }, "aac")),
                        Some(CodecConfig::Opus(c)) => opus_from_mp4(c, a.channels).map(|d| (Dec::Opus(d), "opus")),
                        Some(CodecConfig::Ac3 { .. }) => Some((Dec::Ac3(Box::default()), "ac3")),
                        Some(CodecConfig::Eac3 { .. }) => Some((Dec::Ac3(Box::default()), "eac3")),
                        _ => None,
                    };
                    if let Some((dec, name)) = dec {
                        let tb = f64::from(t.timescale);
                        let o = origin.unwrap_or(0.0);
                        let times = (0..t.samples.len()).map(|k| t.presentation_pts(k).map(|p| p as f64 / tb - o)).collect();
                        found = Some((Track::Mp4 { track: i }, dec, name, times, a.sample_rate, a.channels as usize));
                        break;
                    }
                }
                found.ok_or_else(|| unsupported("no decodable audio track (AAC, Opus, AC-3)"))?
            }
            Container::Mkv { file, track: vt } => {
                use filmcraft_matroska::{Codec, TrackKind};
                let secs = |t: &filmcraft_matroska::Track, p: i64| {
                    let (n, d) = t.timebase;
                    if d == 0 { 0.0 } else { p as f64 * n as f64 / d as f64 }
                };
                let origin = file.tracks.get(*vt).and_then(|t| t.samples.iter().map(|s| s.pts).min().map(|p| secs(t, p))).unwrap_or(0.0);
                let mut found = None;
                for (i, t) in file.tracks.iter().enumerate() {
                    if t.kind != TrackKind::Audio || t.samples.is_empty() {
                        continue;
                    }
                    let a = t.audio.clone().unwrap_or_default();
                    let dec = match &t.codec {
                        Codec::Aac { asc } => filmcraft_aac::Decoder::new(asc).ok().map(|dec| (Dec::Aac { dec, adts: false }, "aac")),
                        Codec::Opus { head } => filmcraft_opus::Decoder::new(head).ok().map(|mut d| {
                            // the codec delay is applied through the timestamps below
                            d.set_trim_pre_skip(false);
                            (Dec::Opus(d), "opus")
                        }),
                        Codec::Ac3 => Some((Dec::Ac3(Box::default()), "ac3")),
                        _ => None,
                    };
                    if let Some((dec, name)) = dec {
                        let delay = t.codec_delay_ns as f64 / 1e9;
                        let times = t.samples.iter().map(|s| Some(secs(t, s.pts) - delay - origin)).collect();
                        let rate = a.output_sampling_frequency.unwrap_or(a.sampling_frequency);
                        found = Some((Track::Mkv { track: i }, dec, name, times, rate, a.channels as usize));
                        break;
                    }
                }
                found.ok_or_else(|| unsupported("no decodable audio track (AAC, Opus, AC-3)"))?
            }
            Container::Ts { file, stream: vs, units } => {
                use filmcraft_mpegts::{Codec as T, Kind};
                let origin = file
                    .streams
                    .get(*vs)
                    .and_then(|s| units.first().and_then(|&u| s.units.get(u)))
                    .and_then(|u| u.pts)
                    .map_or(0.0, |p| p as f64 / 90_000.0);
                let stream = file
                    .find(Kind::Audio, |c| matches!(c, T::AacAdts | T::Ac3 | T::Eac3))
                    .ok_or_else(|| unsupported("no decodable audio stream (ADTS AAC, AC-3)"))?;
                let st = file.streams.get(stream).ok_or_else(|| unsupported("no audio stream"))?;
                let (dec, name) = match st.codec {
                    T::AacAdts => {
                        let first = file.read_unit(&clip.src, stream, 0).map_err(|e| VideoError::Invalid(e.to_string()))?;
                        let h = filmcraft_aac::AdtsHeader::parse(&first).map_err(|e| VideoError::Invalid(format!("ADTS: {e}")))?;
                        let dec = filmcraft_aac::Decoder::from_adts(&h).map_err(|e| VideoError::Unsupported(format!("AAC: {e}")))?;
                        (Dec::Aac { dec, adts: true }, "aac")
                    }
                    T::Eac3 => (Dec::Ac3(Box::default()), "eac3"),
                    _ => (Dec::Ac3(Box::default()), "ac3"),
                };
                let times = st.units.iter().map(|u| u.pts.map(|p| p as f64 / 90_000.0 - origin)).collect();
                // rate and channels come from the first decoded packet
                (Track::Ts { stream }, dec, name, times, 0.0, 0)
            }
            Container::Es { .. } | Container::Mxf { .. } => return Err(unsupported("audio in this container is not decoded")),
        };
        let mut r =
            Reader { clip, track, dec, info: AudioInfo { codec: codec.into(), sample_rate: 0, channels: 0 }, times, next: 0, cursor: 0, from: 0 };
        r.probe_format(rate, channels)?;
        r.seek(0.0);
        Ok(r)
    }

    /// Fix the source rate and channel count: the decoder's when it knows them up front, else the
    /// first packet that decodes (TS, AC-3, rates the container got wrong).
    fn probe_format(&mut self, container_rate: f64, container_channels: usize) -> Result<(), VideoError> {
        let (rate, channels) = match &self.dec {
            Dec::Aac { dec, .. } => (dec.sample_rate(), dec.channels()),
            Dec::Opus(d) => (d.sample_rate(), d.channels()),
            Dec::Ac3(_) => {
                let mut found = None;
                for i in 0..self.times.len().min(MAX_BAD_PACKETS) {
                    let Ok(data) = self.read(i) else { continue };
                    if let Ok(h) = filmcraft_ac3::parse_header(&data) {
                        found = Some((h.sample_rate, h.channels()));
                        break;
                    }
                }
                found.unwrap_or((container_rate as u32, container_channels))
            }
        };
        if !(8_000..=192_000).contains(&rate) {
            return Err(unsupported(format!("audio sample rate {rate} Hz")));
        }
        if channels == 0 || channels > MAX_CHANNELS {
            return Err(unsupported(format!("{channels} audio channels")));
        }
        self.info.sample_rate = rate;
        self.info.channels = channels;
        Ok(())
    }

    pub fn info(&self) -> &AudioInfo {
        &self.info
    }

    fn read(&self, i: usize) -> Result<Vec<u8>, VideoError> {
        let invalid = |e: String| VideoError::Invalid(e);
        match (&self.track, &self.clip.container) {
            (Track::Mp4 { track }, Container::Mp4 { file, .. }) => file.read_sample(&self.clip.src, *track, i).map_err(demux::demux_error),
            (Track::Mkv { track }, Container::Mkv { file, .. }) => file.read_sample(&self.clip.src, *track, i).map_err(|e| invalid(e.to_string())),
            (Track::Ts { stream }, Container::Ts { file, .. }) => file.read_unit(&self.clip.src, *stream, i).map_err(|e| invalid(e.to_string())),
            _ => Err(invalid("audio track mismatch".into())),
        }
    }

    fn to_samples(&self, t: f64) -> i64 {
        let s = (t * f64::from(self.info.sample_rate)).round();
        if s.is_finite() { s.clamp(-(i64::MAX as f64) / 4.0, i64::MAX as f64 / 4.0) as i64 } else { 0 }
    }

    /// Position before `t` seconds: the next block starts at (or just before) `t`.
    pub fn seek(&mut self, t: f64) {
        let t = if t.is_finite() { t.max(0.0) } else { 0.0 };
        // last packet starting at or before t (times are increasing; unknown ones are skipped)
        let k = self.times.partition_point(|x| x.is_none_or(|s| s <= t));
        let target = k.saturating_sub(1);
        let start = target.saturating_sub(PREROLL);
        // the nearest known time at or before the start packet anchors the cursor
        self.next = (0..=start).rev().find(|&i| self.times.get(i).copied().flatten().is_some()).unwrap_or(0);
        self.cursor = self.times.get(self.next).copied().flatten().map_or(0, |s| self.to_samples(s));
        self.from = self.to_samples(t);
        self.dec.reset();
    }

    /// The next decoded block at or after the seek target; None at the end of the track.
    pub fn next_block(&mut self) -> Result<Option<AudioBlock>, VideoError> {
        let mut bad = 0usize;
        loop {
            let Some(&known) = self.times.get(self.next) else { return Ok(None) };
            let i = self.next;
            self.next = i.saturating_add(1);
            if let Some(s) = known {
                // follow container timestamps only past a jitter tolerance: Matroska rounds to
                // its timestamp scale (usually 1 ms), so packets keep the running sample count
                let at = self.to_samples(s);
                if at.abs_diff(self.cursor) > u64::from(self.info.sample_rate / RESYNC_DIV) {
                    self.cursor = at;
                }
            }
            let decoded = match self.read(i) {
                Ok(data) => {
                    let dec = &mut self.dec;
                    guarded("audio decode", || dec.decode(&data).map_err(VideoError::Decode))
                }
                Err(e) => Err(e),
            };
            let planar = match decoded {
                Ok(Some(p)) => p,
                Ok(None) => continue,
                Err(e) => {
                    bad = bad.saturating_add(1);
                    if bad >= MAX_BAD_PACKETS {
                        return Err(e);
                    }
                    continue;
                }
            };
            bad = 0;
            let ch = self.info.channels;
            let n = planar.iter().map(Vec::len).min().unwrap_or(0).min(MAX_PACKET_SAMPLES);
            let start = self.cursor;
            self.cursor = self.cursor.saturating_add(n as i64);
            if n == 0 || self.cursor <= self.from {
                continue;
            }
            let skip = usize::try_from(self.from.saturating_sub(start)).unwrap_or(0).min(n);
            let mut samples = Vec::with_capacity((n - skip) * ch);
            for k in skip..n {
                for c in 0..ch {
                    // a packet with fewer channels than the stream (AC-3 mode change): silence
                    samples.push(planar.get(c).and_then(|p| p.get(k)).copied().unwrap_or(0.0));
                }
            }
            let first = start.saturating_add(skip as i64);
            self.from = self.cursor;
            return Ok(Some(AudioBlock { time: first as f64 / f64::from(self.info.sample_rate), samples }));
        }
    }
}

/// An Opus decoder from an MP4 `dOps` box (pre-skip comes through the edit list).
fn opus_from_mp4(c: &filmcraft_isobmff::OpusConfig, entry_channels: u32) -> Option<filmcraft_opus::Decoder> {
    let channels = if c.output_channels > 0 { usize::from(c.output_channels) } else { entry_channels as usize };
    let mut head = filmcraft_opus::OpusHead::simple(channels.clamp(1, 2)).ok()?;
    if c.channel_mapping_family != 0 {
        let (&streams, rest) = c.channel_mapping.split_first()?;
        let (&coupled, mapping) = rest.split_first()?;
        head.channels = channels;
        head.mapping_family = c.channel_mapping_family;
        head.streams = usize::from(streams);
        head.coupled_streams = usize::from(coupled);
        head.mapping = mapping.to_vec();
    }
    head.pre_skip = c.pre_skip;
    head.output_gain = c.output_gain;
    head.input_sample_rate = c.input_sample_rate;
    let mut d = filmcraft_opus::Decoder::from_head(head, 48_000).ok()?;
    d.set_trim_pre_skip(false);
    Some(d)
}
