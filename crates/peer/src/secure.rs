//! The encrypted, mutually authenticated channel between two devices, over TCP.
//!
//! Handshake (own design, after the Noise / SIGMA pattern; Sources: RFC 7748 X25519, RFC 8032
//! Ed25519, RFC 5869 HKDF, RFC 8439 ChaCha20-Poly1305):
//! 1. Each side sends `Hello {device, key, eph}`: its device id, its Ed25519 public key and a
//!    fresh X25519 public key.
//! 2. `transcript = SHA-256(label ‖ hello_initiator ‖ hello_responder)`. Each side signs
//!    `transcript ‖ role` with its Ed25519 key and checks the other's signature against the key it
//!    sent. Nobody in the middle can swap the ephemeral keys without a device's private key.
//! 3. HKDF-SHA-256 over the X25519 shared secret (salt = transcript) gives one key per direction
//!    and the pairing code. The code covers both devices' keys, so a device in the middle (which
//!    must use its own key) makes the two screens show different codes.
//!
//! Whether the other device may talk is the caller's decision: a paired device's key must match
//! the stored one ([`crate::identity::Store`]); a pairing needs the people to compare the codes.
//!
//! Frames: a 4-byte big-endian length, then ChaCha20-Poly1305 ciphertext; the nonce is a per-
//! direction counter, so a replayed, reordered or altered frame fails to decrypt.

use std::io::{Read, Write};
use std::net::TcpStream;

use base64::Engine as _;
use base64::engine::general_purpose::STANDARD as B64;
use chacha20poly1305::aead::{Aead, KeyInit};
use chacha20poly1305::{ChaCha20Poly1305, Key, Nonce};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::identity::{Identity, random, verify};
use crate::{Error, Result};

const LABEL: &[u8] = b"lcc-peer-v1";
/// The largest handshake message.
const MAX_HELLO: usize = 4096;
/// The largest frame (a batch of catalog records).
pub const MAX_FRAME: usize = 64 << 20;

#[derive(Serialize, Deserialize)]
struct Hello {
    device: String,
    key: String,
    eph: String,
}

#[derive(Serialize, Deserialize)]
struct Auth {
    sig: String,
}

fn write_raw(s: &mut TcpStream, bytes: &[u8]) -> Result<()> {
    let len = u32::try_from(bytes.len()).map_err(|_| Error::Protocol("message too large".into()))?;
    s.write_all(&len.to_be_bytes())?;
    s.write_all(bytes)?;
    s.flush()?;
    Ok(())
}

fn read_raw(s: &mut TcpStream, max: usize) -> Result<Vec<u8>> {
    let mut len = [0u8; 4];
    s.read_exact(&mut len)?;
    let len = u32::from_be_bytes(len) as usize;
    if len > max {
        return Err(Error::Protocol(format!("message of {len} bytes refused")));
    }
    let mut buf = vec![0u8; len];
    s.read_exact(&mut buf)?;
    Ok(buf)
}

/// One side of an established channel.
pub struct Channel {
    stream: TcpStream,
    send: ChaCha20Poly1305,
    recv: ChaCha20Poly1305,
    sent: u64,
    received: u64,
    /// The other device's id and public key (proved by its signature).
    pub peer_device: String,
    pub peer_key: String,
    /// The six-digit code both screens show while pairing.
    pub code: String,
}

impl std::fmt::Debug for Channel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Channel").field("peer_device", &self.peer_device).finish_non_exhaustive()
    }
}

fn nonce(counter: u64) -> [u8; 12] {
    let mut n = [0u8; 12];
    n[4..].copy_from_slice(&counter.to_be_bytes());
    n
}

impl Channel {
    /// Runs the handshake; `initiator` is the side that connected.
    pub fn handshake(mut stream: TcpStream, me: &Identity, initiator: bool) -> Result<Channel> {
        let eph_secret = x25519_dalek::StaticSecret::from(random::<32>()?);
        let eph_public = x25519_dalek::PublicKey::from(&eph_secret);
        let mine = serde_json::to_vec(&Hello { device: me.device_id.clone(), key: me.public_key()?, eph: B64.encode(eph_public.as_bytes()) })
            .map_err(|e| Error::Protocol(e.to_string()))?;
        let theirs = if initiator {
            write_raw(&mut stream, &mine)?;
            read_raw(&mut stream, MAX_HELLO)?
        } else {
            let t = read_raw(&mut stream, MAX_HELLO)?;
            write_raw(&mut stream, &mine)?;
            t
        };
        let hello: Hello = serde_json::from_slice(&theirs).map_err(|_| Error::Protocol("bad hello".into()))?;
        if hello.device == me.device_id {
            return Err(Error::Protocol("connected to itself".into()));
        }
        let eph: [u8; 32] = B64.decode(&hello.eph).ok().and_then(|b| b.try_into().ok()).ok_or_else(|| Error::Protocol("bad key exchange".into()))?;
        let (first, second) = if initiator { (&mine, &theirs) } else { (&theirs, &mine) };
        let transcript: [u8; 32] = Sha256::new().chain_update(LABEL).chain_update(first).chain_update(second).finalize().into();

        let role = |init: bool| -> Vec<u8> { [&transcript[..], &[u8::from(init)]].concat() };
        let my_sig = serde_json::to_vec(&Auth { sig: B64.encode(me.sign(&role(initiator))?) }).map_err(|e| Error::Protocol(e.to_string()))?;
        write_raw(&mut stream, &my_sig)?;
        let their_sig: Auth = serde_json::from_slice(&read_raw(&mut stream, MAX_HELLO)?).map_err(|_| Error::Protocol("bad signature".into()))?;
        let sig = B64.decode(&their_sig.sig).map_err(|_| Error::Protocol("bad signature".into()))?;
        if !verify(&hello.key, &role(!initiator), &sig) {
            return Err(Error::Protocol("the other device's signature doesn't check out".into()));
        }

        let shared = eph_secret.diffie_hellman(&x25519_dalek::PublicKey::from(eph));
        if !shared.was_contributory() {
            return Err(Error::Protocol("bad key exchange".into()));
        }
        let hk = hkdf::Hkdf::<Sha256>::new(Some(&transcript), shared.as_bytes());
        let mut keys = [0u8; 68];
        hk.expand(b"keys+code", &mut keys).map_err(|_| Error::Protocol("key derivation".into()))?;
        let (i2r, rest) = keys.split_at(32);
        let (r2i, code) = rest.split_at(32);
        let code = u32::from_be_bytes([code[0], code[1], code[2], code[3]]) % 1_000_000;
        let (send, recv) = if initiator { (i2r, r2i) } else { (r2i, i2r) };
        Ok(Channel {
            stream,
            send: ChaCha20Poly1305::new(Key::from_slice(send)),
            recv: ChaCha20Poly1305::new(Key::from_slice(recv)),
            sent: 0,
            received: 0,
            peer_device: hello.device,
            peer_key: hello.key,
            code: format!("{code:06}"),
        })
    }

    pub fn send(&mut self, plaintext: &[u8]) -> Result<()> {
        if plaintext.len() > MAX_FRAME {
            return Err(Error::Protocol("frame too large".into()));
        }
        let n = nonce(self.sent);
        self.sent = self.sent.checked_add(1).ok_or_else(|| Error::Protocol("channel exhausted".into()))?;
        let ct = self.send.encrypt(Nonce::from_slice(&n), plaintext).map_err(|_| Error::Protocol("encryption failed".into()))?;
        write_raw(&mut self.stream, &ct)
    }

    pub fn recv(&mut self) -> Result<Vec<u8>> {
        let ct = read_raw(&mut self.stream, MAX_FRAME + 16)?;
        let n = nonce(self.received);
        self.received = self.received.checked_add(1).ok_or_else(|| Error::Protocol("channel exhausted".into()))?;
        self.recv.decrypt(Nonce::from_slice(&n), ct.as_ref()).map_err(|_| Error::Protocol("a message failed to decrypt".into()))
    }

    pub fn send_json<T: Serialize>(&mut self, value: &T) -> Result<()> {
        let bytes = serde_json::to_vec(value).map_err(|e| Error::Protocol(e.to_string()))?;
        self.send(&bytes)
    }

    pub fn recv_json<T: for<'de> Deserialize<'de>>(&mut self) -> Result<T> {
        let bytes = self.recv()?;
        serde_json::from_slice(&bytes).map_err(|e| Error::Protocol(format!("unexpected message: {e}")))
    }

    /// Limits how long a read may wait.
    pub fn set_timeout(&self, t: Option<std::time::Duration>) -> Result<()> {
        self.stream.set_read_timeout(t)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::TcpListener;

    fn pair() -> (Result<Channel>, Result<Channel>, Identity, Identity) {
        let a = Identity::generate("A").unwrap();
        let b = Identity::generate("B").unwrap();
        let l = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = l.local_addr().unwrap();
        let b2 = b.clone();
        let t = std::thread::spawn(move || Channel::handshake(l.accept().unwrap().0, &b2, false));
        let ca = Channel::handshake(TcpStream::connect(addr).unwrap(), &a, true);
        (ca, t.join().unwrap(), a, b)
    }

    #[test]
    fn both_sides_agree_and_messages_flow_both_ways() {
        let (ca, cb, a, b) = pair();
        let (mut ca, mut cb) = (ca.unwrap(), cb.unwrap());
        assert_eq!(ca.code, cb.code);
        assert_eq!(ca.code.len(), 6);
        assert_eq!(ca.peer_device, b.device_id);
        assert_eq!(cb.peer_key, a.public_key().unwrap());
        ca.send(b"one").unwrap();
        ca.send(b"two").unwrap();
        assert_eq!(cb.recv().unwrap(), b"one");
        assert_eq!(cb.recv().unwrap(), b"two");
        cb.send(b"back").unwrap();
        assert_eq!(ca.recv().unwrap(), b"back");
    }

    #[test]
    fn a_tampered_frame_fails_to_decrypt() {
        let (ca, cb, _, _) = pair();
        let (ca, mut cb) = (ca.unwrap(), cb.unwrap());
        // bypass the cipher: a raw frame of garbage where a sealed one belongs
        let mut raw = ca.stream.try_clone().unwrap();
        write_raw(&mut raw, &[7u8; 40]).unwrap();
        assert!(cb.recv().is_err());
    }

    #[test]
    fn garbage_instead_of_a_handshake_is_an_error() {
        let me = Identity::generate("A").unwrap();
        for junk in [&b"\x00\x00\x00\x05hello"[..], b"\xff\xff\xff\xff", b"\x00\x00\x00\x02{}"] {
            let l = TcpListener::bind("127.0.0.1:0").unwrap();
            let addr = l.local_addr().unwrap();
            let junk = junk.to_vec();
            let t = std::thread::spawn(move || {
                let mut s = TcpStream::connect(addr).unwrap();
                let _ = s.write_all(&junk);
            });
            let r = Channel::handshake(l.accept().unwrap().0, &me, false);
            t.join().unwrap();
            assert!(r.is_err());
        }
    }
}
