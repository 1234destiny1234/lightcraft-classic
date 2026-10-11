//! End to end: settings → pages → PDF / JPEG.

use dac_layout::render::PhotoSource;
use dac_layout::tokens::PhotoInfo;
use dac_raster::Rgba8;

use crate::job::builtin_templates;
use crate::output::{PdfOptions, to_jpeg, to_pdf};
use crate::text::ShapedText;
use crate::{LayoutStyle, PrintSettings};

struct Gradient;
impl PhotoSource for Gradient {
    fn image(&self, photo: &str, long: usize) -> Result<Rgba8, String> {
        if photo == "missing" {
            return Err("not found".into());
        }
        let long = long.clamp(8, 600);
        Ok(Rgba8::from_fn(long, long * 2 / 3, |x, y| [(x * 255 / long) as u8, (y * 255 / long) as u8, 128, 255]))
    }
    fn info(&self, photo: &str) -> PhotoInfo {
        PhotoInfo { filename: format!("{photo}.dng"), title: "Ünïcödé 写真".into(), ..PhotoInfo::default() }
    }
}

fn template(name: &str) -> PrintSettings {
    builtin_templates().into_iter().find(|(n, _)| n == name).map(|(_, s)| s).unwrap_or_default()
}

fn ids(n: usize) -> Vec<String> {
    (0..n).map(|i| format!("IMG_{i:04}")).collect()
}

fn pdf_of(s: &PrintSettings, photos: &[String]) -> (lopdf::Document, Vec<u8>, Vec<String>) {
    let doc = s.build(photos).unwrap();
    let out = to_pdf(&doc, s, &Gradient, &ShapedText::new(), &PdfOptions::default(), &mut |_, _| true).unwrap();
    let bytes = out.files.into_iter().next().unwrap();
    (lopdf::Document::load_mem(&bytes).unwrap(), bytes, out.warnings)
}

fn has_embedded_font(pdf: &lopdf::Document) -> bool {
    pdf.objects.values().any(|o| o.as_dict().is_ok_and(|d| d.has(b"FontFile2") || d.has(b"FontFile3")))
}

#[test]
fn contact_sheet_2x2_to_pdf_with_fonts_and_marks() {
    let mut s = template("2×2 Cells");
    s.options.photo_info = Some("{Filename}".into());
    s.options.page_numbers = true;
    s.options.page_info = true;
    s.options.crop_marks = true;
    let (pdf, _, warnings) = pdf_of(&s, &ids(6));
    assert!(warnings.is_empty(), "{warnings:?}");
    let pages = pdf.get_pages();
    assert_eq!(pages.len(), 2);
    assert!(has_embedded_font(&pdf));
    let first = pdf.get_object(*pages.get(&1).unwrap()).unwrap().as_dict().unwrap();
    let media = first.get(b"MediaBox").unwrap().as_array().unwrap();
    // crop marks need room outside the trim: the media box is larger than Letter
    assert!(media[2].as_float().unwrap() > 612.0 + 40.0);
    assert!(first.has(b"TrimBox"));
    let text: String = pdf.extract_text(&[1]).unwrap().chars().filter(|c| !c.is_whitespace()).collect();
    assert!(text.contains("IMG_0000.dng"), "{text}");
    assert!(text.contains("Page1of2"), "{text}");
}

#[test]
fn picture_package_and_custom_package_to_pdf() {
    let (pdf, _, _) = pdf_of(&template("(1) 4×6, (6) 2×3"), &ids(2));
    assert!(pdf.get_pages().len() >= 2);
    let mut custom = template("Custom Centered");
    custom.options.identity_plate = Some(crate::job::IdentityPlate { text: "Studio 東京".into(), ..Default::default() });
    let (pdf, bytes, _) = pdf_of(&custom, &ids(3));
    assert_eq!(pdf.get_pages().len(), 1);
    assert!(has_embedded_font(&pdf));
    // one image object per distinct photo and size
    let images = pdf
        .objects
        .values()
        .filter(|o| o.as_stream().is_ok_and(|s| s.dict.get(b"Subtype").is_ok_and(|v| v.as_name().is_ok_and(|n| n == b"Image"))))
        .count();
    assert_eq!(images, 3);
    assert!(bytes.starts_with(b"%PDF-"));
}

#[test]
fn missing_photos_warn_and_cancel_stops() {
    let s = template("2×2 Cells");
    let doc = s.build(&["missing".to_string(), "IMG_1".to_string()]).unwrap();
    let out = to_pdf(&doc, &s, &Gradient, &ShapedText::new(), &PdfOptions::default(), &mut |_, _| true).unwrap();
    assert_eq!(out.warnings.len(), 1);
    assert_eq!(to_pdf(&doc, &s, &Gradient, &ShapedText::new(), &PdfOptions::default(), &mut |_, _| false).unwrap_err(), crate::PrintError::Cancelled);
}

#[test]
fn jpeg_pages_at_print_resolution() {
    let mut s = template("4×5 Contact Sheet");
    s.job.dpi = 50.0;
    let doc = s.build(&ids(21)).unwrap();
    let out = to_jpeg(&doc, &s, &Gradient, &ShapedText::new(), &mut |_, _| true).unwrap();
    assert_eq!(out.files.len(), 2);
    let info = dac_pdf::jpeg_info(&out.files[0]).unwrap();
    assert_eq!((info.width, info.height), (425, 550)); // 8.5×11 in at 50 ppi
}

#[test]
fn single_image_grid_style_survives_round_trip() {
    let s = PrintSettings { layout: LayoutStyle::SingleImage { grid: dac_layout::grid::Grid::new(3, 2) }, ..PrintSettings::default() };
    let back = PrintSettings::from_json(&s.to_json().unwrap()).unwrap();
    assert_eq!(back.build(&ids(7)).unwrap().pages.len(), 2);
}

/// Opt-in: a real CUPS queue (e.g. `cups-pdf`) at the IPP URI in `<PREFIX>_CUPS_TEST_PRINTER`
/// (`ipp://localhost:631/printers/PDF`). Reads its paper sizes, then prints a contact sheet PDF.
/// Skipped when the variable is unset (see docs/print.md → CUPS test printer).
#[cfg(not(target_arch = "wasm32"))]
#[test]
fn cups_test_printer_receives_a_job() {
    let Some(uri) = dac_brand::env("CUPS_TEST_PRINTER").filter(|u| !u.trim().is_empty()) else {
        eprintln!("skipped: set {} to an IPP printer URI to run", dac_brand::env_var("CUPS_TEST_PRINTER"));
        return;
    };
    let info = crate::ipp::printer_info(&uri).unwrap();
    assert!(!info.media.is_empty(), "the printer reports paper sizes: {info:?}");
    assert!(info.accepts("application/pdf"), "{:?}", info.formats);
    let mut s = template("2×2 Cells");
    if let Some(m) = info.media.first() {
        s.set_paper(m.size, false);
        s.fit_printer_margins(m.margins);
    }
    let doc = s.build(&ids(4)).unwrap();
    let mut opt = send_opts();
    opt.wait = std::time::Duration::from_secs(60);
    opt.poll = std::time::Duration::from_millis(500);
    let sent = crate::send::send_to_printer(&uri, &doc, &s, &Gradient, &ShapedText::new(), &opt, &mut |_, _| true).unwrap();
    assert!(sent.jobs.iter().all(|j| *j > 0), "job ids: {sent:?}");
    eprintln!("CUPS job {:?}: {:?}", sent.jobs, sent.state);
}

fn send_opts() -> crate::send::SendOptions {
    crate::send::SendOptions {
        user: "tester".into(),
        title: "Contact sheet".into(),
        pdf: PdfOptions::default(),
        wait: std::time::Duration::from_secs(5),
        poll: std::time::Duration::from_millis(20),
    }
}

/// The IPP CI path: page setup from the printer's paper, a 2×2 contact sheet as PDF with copies,
/// the job followed to "completed", all against the virtual printer.
#[test]
fn virtual_ipp_printer_receives_a_contact_sheet() {
    use crate::testprinter::{TestPrinter, TestPrinterConfig};
    let printer = TestPrinter::start(TestPrinterConfig::default()).unwrap();
    let info = crate::ipp::printer_info(printer.uri()).unwrap();
    let a4 = info.media.iter().find(|m| m.name == "iso_a4_210x297mm").unwrap().clone();
    assert!((a4.margins[1] - 8.5).abs() < 0.1, "{:?}", a4.margins);
    let mut s = template("2×2 Cells");
    s.set_paper(a4.size, false);
    s.page.margins = dac_layout::Insets::all(0.0);
    s.fit_printer_margins(a4.margins);
    assert!((s.page.margins.top - 14.17).abs() < 0.1);
    s.job.copies = 3;
    let doc = s.build(&ids(4)).unwrap();
    let sent = crate::send::send_to_printer(printer.uri(), &doc, &s, &Gradient, &ShapedText::new(), &send_opts(), &mut |_, _| true).unwrap();
    assert_eq!(sent.format, "application/pdf");
    assert_eq!(sent.jobs, vec![1]);
    assert_eq!(sent.state.as_ref().map(|j| j.state), Some(crate::ipp::JobState::COMPLETED));
    let jobs = printer.jobs();
    assert_eq!(jobs.len(), 1);
    assert_eq!((jobs[0].copies, jobs[0].user.as_str(), jobs[0].format.as_str()), (3, "tester", "application/pdf"));
    let pdf = lopdf::Document::load_mem(&jobs[0].document).unwrap();
    assert_eq!(pdf.get_pages().len(), 1);
}

#[test]
fn virtual_printer_without_pdf_gets_jpeg_pages_and_aborts_are_errors() {
    use crate::testprinter::{TestPrinter, TestPrinterConfig};
    let jpeg_only = TestPrinter::start(TestPrinterConfig { formats: vec!["image/jpeg".into()], ..TestPrinterConfig::default() }).unwrap();
    let s = template("2×2 Cells");
    let doc = s.build(&ids(6)).unwrap();
    let sent = crate::send::send_to_printer(jpeg_only.uri(), &doc, &s, &Gradient, &ShapedText::new(), &send_opts(), &mut |_, _| true).unwrap();
    assert_eq!((sent.format.as_str(), sent.jobs.len(), sent.pages), ("image/jpeg", 2, 2));
    assert!(jpeg_only.jobs().iter().all(|j| j.document.starts_with(&[0xFF, 0xD8])));

    let raster = TestPrinter::start(TestPrinterConfig { formats: vec!["image/pwg-raster".into()], ..TestPrinterConfig::default() }).unwrap();
    let e = crate::send::send_to_printer(raster.uri(), &doc, &s, &Gradient, &ShapedText::new(), &send_opts(), &mut |_, _| true).unwrap_err();
    assert!(e.to_string().contains("neither PDF nor JPEG"), "{e}");
    assert!(raster.jobs().is_empty());

    let aborts = TestPrinter::start(TestPrinterConfig { final_state: crate::ipp::JobState::ABORTED, ..TestPrinterConfig::default() }).unwrap();
    let e = crate::send::send_to_printer(aborts.uri(), &doc, &s, &Gradient, &ShapedText::new(), &send_opts(), &mut |_, _| true).unwrap_err();
    assert!(e.to_string().contains("aborted"), "{e}");
}

#[test]
fn virtual_printer_survives_garbage_and_copies_are_checked() {
    use std::io::{Read, Write};
    let printer = crate::testprinter::TestPrinter::start(Default::default()).unwrap();
    let port = crate::ipp::IppUri::parse(printer.uri()).unwrap().port;
    for junk in [&b"GET / HTTP/1.1\r\n\r\n"[..], b"POST / HTTP/1.1\r\nContent-Length: 3\r\n\r\nabc", b"\0\0\0\0"] {
        let mut s = std::net::TcpStream::connect(("127.0.0.1", port)).unwrap();
        s.write_all(junk).unwrap();
        let _ = s.shutdown(std::net::Shutdown::Write);
        let mut out = Vec::new();
        let _ = s.read_to_end(&mut out);
    }
    assert!(crate::ipp::printer_info(printer.uri()).is_ok(), "still answering");
    assert!(crate::ipp::job_state(printer.uri(), "me", 77).is_err(), "unknown job");
    let mut s = template("2×2 Cells");
    s.job.copies = 0;
    assert!(s.validate().is_err());
}
