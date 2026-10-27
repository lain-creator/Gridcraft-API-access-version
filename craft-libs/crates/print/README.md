# craft-print

Submit a print-ready PDF to the system spooler. Used by PdfCraft and DesignCraft;
there are no dependencies on either app or on a UI framework. Requires Rust 1.95.

```rust,no_run
let job = craft_print::Job { title: "Report".into(), ..Default::default() };
let receipt = craft_print::submit(&pdf_bytes, &job)?;
```

The caller generates the PDF, checks document permissions, and handles page selection,
imposition, marks and bleed. This crate discovers printers, exposes CUPS driver options,
opens Windows printer preferences and submits jobs with copies, collation, duplex and colour.
`command_preview` returns diagnostics without creating files or starting a process.

- **macOS/Linux:** CUPS `lpstat`, `lpoptions` and `lp`. Jobs stream through stdin. Exact-size
  sheets use `fit-to-page=false`; shrink-to-fit uses IPP `print-scaling=auto-fit`.
- **Windows:** inbox Windows PowerShell, `Windows.Data.Pdf` and `System.Drawing.Printing`.
  Each sheet is rasterized, with paper and orientation selected per page. Exact-size mode
  preserves physical size and clips hardware margins. Shrink-to-fit uses matching driver
  paper or its default, centers in the printable area and never enlarges smaller sheets.
  Long-edge duplex uses `Vertical`; short-edge uses
  `Horizontal`. Driver-specific defaults remain in Windows printer preferences.
- **Browser/other targets:** discovery is empty and submission reports an unsupported
  platform; callers can offer PDF export instead.

Windows jobs briefly use a random, exclusively created temporary file, removed on drop
on both success and failure. CUPS jobs never use a PDF temporary file. Printing is synchronous;
UI callers should submit on a worker. Receipts are backend messages, not portable job IDs.
`Job::duplex = None` preserves the queue's duplex default; `Some(Duplex::Off)` forces
simplex. `Job::scaling` defaults to `Scaling::Exact`; callers exporting arbitrary document
sizes can choose `Scaling::ShrinkToFit`. Exact mode reports a driver paper-size mismatch.
Windows raster allocation is bounded to 64 million pixels per sheet and 32768 per dimension;
large sheets use lower raster resolution while retaining physical size. The backend supports
sheets up to 128 inches and reports invalid sizes. It does not provide PostScript output,
separations, trapping, colour-managed device rasterization or PDF passthrough on Windows.

`cargo test -p craft-print` tests CUPS transport (including pipe-sized jobs and refusal),
printer parsing, PPD options and argument mapping. On Windows it also parses the embedded
PowerShell, checks physical sheet geometry, duplex and raster bounds without hardware,
renders a mixed-orientation PDF through WinRT, and attempts submission to a temporary paused
queue. The queue portion explicitly skips if the driver or administrative rights are absent;
rendering and geometry tests do not skip. Windows CI installs the inbox test driver and sets
`CRAFT_PRINT_REQUIRE_QUEUE=1`, making queue preparation/submission mandatory on its runner.
Physical printer fidelity remains a hardware check.

The cross-platform GitHub Actions workflow is defined in `.github/workflows/print.yml`.
It runs formatting, Clippy and this crate's own tests on Windows, Linux and macOS when print
code changes, and supports manual runs. The Windows job executes the WinRT and spooler
regressions above; physical printer fidelity still requires hardware verification.

Windows discovery reports driver duplex support in `Printer::two_sided`; CUPS leaves it unknown.
Callers can warn when a Windows driver ignores the job’s duplex choice.
