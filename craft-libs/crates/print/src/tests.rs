use super::*;
use crate as spool;

#[test]
fn spooler_arguments_and_printer_list() {
    let out = "printer Office_Laser is idle.  enabled since Thu Oct  1 09:00:00 2026\nprinter Label_Writer disabled since …\nsystem default destination: Office_Laser\n";
    assert_eq!(
        parse_lpstat(out),
        [
            spool::Printer { name: "Office_Laser".into(), default: true, two_sided: None },
            spool::Printer { name: "Label_Writer".into(), default: false, two_sided: None }
        ]
    );
    assert!(parse_lpstat("lpstat: No destinations added.\nno system default destination\n").is_empty());
    let job = Job {
        printer: Some("Office_Laser".into()),
        copies: 3,
        collate: false,
        duplex: Some(Duplex::LongEdge),
        grayscale: true,
        title: "memo.pdf".into(),
        options: Vec::new(),
        ..Job::default()
    };
    assert_eq!(
        lp_args(&job).join(" "),
        "-d Office_Laser -n 3 -t memo.pdf -o collate=false -o sides=two-sided-long-edge -o print-color-mode=monochrome -o fit-to-page=false"
    );
    assert_eq!(lp_args(&Job::default())[0], "-n", "no -d: the default printer");
}

#[test]
fn lpstat_output_is_untranslated() {
    // A localized lpstat (here Polish) is unreadable to parse_lpstat...
    assert!(parse_lpstat("drukarka Office_Laser jest bezczynna.\ndomyślny cel systemowy: Office_Laser\n").is_empty());
    // ...so the command must force the C locale, including the SOFTWARE switch macOS CUPS needs.
    let cmd = spool::lpstat_command();
    let envs: Vec<_> = cmd.get_envs().map(|(k, v)| (k.to_string_lossy().into_owned(), v.map(|v| v.to_string_lossy().into_owned()))).collect();
    for key in ["LC_ALL", "LANG"] {
        assert!(envs.contains(&(key.into(), Some("C".into()))), "{key}=C missing: {envs:?}");
    }
    assert!(envs.iter().any(|(k, v)| k == "SOFTWARE" && v.as_deref().is_some_and(|v| !v.is_empty())), "SOFTWARE missing: {envs:?}");
}

#[test]
fn driverless_printers_join_the_list_without_a_queue() {
    // A driverless network printer (IPP, discovered on the LAN) has no queue, so `lpstat -p`
    // never lists it — yet GTK's print dialog shows it and `lp` prints to it through a temporary
    // queue. Only `lpstat -e` names it, and the Print dialog used to show Save as PDF alone.
    let queues = "printer Office_Laser is idle.  enabled since Thu Oct  1 09:00:00 2026\nsystem default destination: Office_Laser\n";
    assert_eq!(
        spool::printers_parsed(queues, Some("Basement_Color\nOffice_Laser\n")),
        [
            spool::Printer { name: "Office_Laser".into(), default: true, two_sided: None },
            spool::Printer { name: "Basement_Color".into(), default: false, two_sided: None },
        ],
        "queues first, then the driverless destination; the queue listed twice is not doubled"
    );
    // An old spooler without `-e` (CUPS < 1.7) keeps its queues; only the driverless names are lost.
    assert_eq!(spool::printers_parsed(queues, None), [spool::Printer { name: "Office_Laser".into(), default: true, two_sided: None }]);
    // The default can itself be driverless (`lpoptions -d` names a destination no queue exists
    // for): it keeps its marker, so the dialog preselects the user's default, not the first queue.
    assert_eq!(
        spool::printers_parsed("system default destination: Basement_Color\n", Some("Basement_Color\n")),
        [spool::Printer { name: "Basement_Color".into(), default: true, two_sided: None }],
        "a default destination without a queue is still the default"
    );
    assert!(spool::parse_lpstat_e("\n \n").is_empty(), "no destinations is empty, not [\"\"]");
    assert_eq!(spool::parse_lpstat_e("A\r\nB \n"), ["A".to_string(), "B".to_string()], "CRLF and stray spaces are trimmed");
}

/// Set on the child process when a test runs this test binary as a stand-in for `lp`.
const STAND_IN_LP: &str = "CRAFT_STAND_IN_LP";

/// Not a test of its own: [`stand_in_lp`] re-runs this binary with only this test selected, and it
/// then plays `lp`. It records its arguments and stdin in the folder named by [`STAND_IN_LP`], or,
/// when the folder is `refuse`, exits without reading stdin, the way `lp` refuses an unknown
/// printer.
#[test]
fn stand_in_for_lp() {
    let Some(record) = std::env::var_os(STAND_IN_LP) else { return };
    if record == "refuse" {
        eprintln!("lp: The printer or class does not exist.");
        std::process::exit(1);
    }
    let record = std::path::PathBuf::from(record);
    let args: Vec<String> = std::env::args().skip_while(|a| a != "--").skip(1).collect();
    let mut stdin = Vec::new();
    std::io::Read::read_to_end(&mut std::io::stdin(), &mut stdin).unwrap();
    std::fs::write(record.join("args"), args.join(" ")).unwrap();
    std::fs::write(record.join("stdin"), stdin).unwrap();
}

/// A spooler command that runs [`stand_in_for_lp`] with `record` (see there).
fn stand_in_lp(record: &std::ffi::OsStr) -> std::process::Command {
    let mut c = std::process::Command::new(std::env::current_exe().unwrap());
    c.args(["--exact", "tests::stand_in_for_lp", "--nocapture", "--"]).env(STAND_IN_LP, record);
    c
}

#[test]
fn print_jobs_reach_lp_on_stdin_never_through_a_shared_temp_folder() {
    // The job used to be written to `<temp>/craft-print-<pid>/job-<time>.pdf` and handed to `lp`
    // by name. In a shared /tmp another local user can predict that folder, create it first (or
    // plant a symlink there) and so read every printed document or swap the file before `lp`
    // reads it. Piping the job to `lp` leaves nothing on disk.
    static N: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
    let record =
        std::env::temp_dir().join(format!("craft-print-test-{}-{}", std::process::id(), N.fetch_add(1, std::sync::atomic::Ordering::Relaxed)));
    std::fs::create_dir(&record).unwrap();
    // Bigger than any pipe buffer, so the spooler must read while the job is still being written.
    let pdf: Vec<u8> = b"%PDF-1.7 synthetic print job\n".iter().copied().cycle().take(3 << 20).collect();
    let job = Job { title: "memo.pdf".into(), ..Job::default() };
    let sent = spool::submit_via(stand_in_lp(record.as_os_str()), &pdf, &job);
    let args = std::fs::read_to_string(record.join("args"));
    let stdin = std::fs::read(record.join("stdin"));
    let _ = std::fs::remove_dir_all(&record);
    assert!(sent.is_ok(), "{sent:?}");
    assert_eq!(args.unwrap(), "-n 1 -t memo.pdf -o collate=true -o fit-to-page=false", "no file argument: lp reads the job from stdin");
    assert!(stdin.unwrap() == pdf, "lp receives the whole job on stdin");
    let predictable = std::env::temp_dir().join(format!("craft-print-{}", std::process::id()));
    assert!(!predictable.exists(), "nothing is created at {predictable:?}, a name other local users can predict");
}

#[test]
fn a_refused_print_job_reports_the_spoolers_message() {
    // `lp` refuses an unknown printer without reading the job; the user sees why, not a broken pipe.
    let pdf = vec![b'%'; 3 << 20];
    let err = spool::submit_via(stand_in_lp("refuse".as_ref()), &pdf, &Job::default());
    assert_eq!(err, Err(Error::Spool("lp: The printer or class does not exist.".into())));
}

#[test]
fn the_spoolers_reply_drops_the_file_count_of_a_job_sent_on_stdin() {
    assert_eq!(
        spool::job_message(
            b"request id is Office_Laser-12 (0 file(s))
"
        ),
        "request id is Office_Laser-12"
    );
    assert_eq!(
        spool::job_message(
            b"request id is Office_Laser-13 (1 file(s))
"
        ),
        "request id is Office_Laser-13 (1 file(s))"
    );
    assert_eq!(spool::job_message(b""), "");
}

#[test]
fn windows_printers_carry_one_default_and_keep_their_names() {
    // Windows marks the system default with a `*` no queue name may contain; names hold spaces,
    // parentheses and non-ASCII letters, and the lines end with CRLF.
    let printers = parse_windows_printers("*Microsoft Print to PDF\r\nHP LaserJet 400 (Copy 2)\r\nКопир Canon iR\r\n\r\n*XPS via USB\r\n");
    assert_eq!(printers[0], Printer { name: "Microsoft Print to PDF".into(), default: true, two_sided: None });
    assert_eq!(printers[1], Printer { name: "HP LaserJet 400 (Copy 2)".into(), default: false, two_sided: None });
    assert_eq!(printers[2], Printer { name: "Копир Canon iR".into(), default: false, two_sided: None });
    // A spooler marking two defaults: the first still wins, so the dialog has one to preselect.
    assert_eq!(printers[3], Printer { name: "XPS via USB".into(), default: false, two_sided: None });
    assert_eq!(parse_windows_printers("\r\n"), Vec::new());
    let sided = parse_windows_printers("*Office PCL\t2\r\nOffice PS\t1\r\nTab\tName\t2\r\n");
    assert_eq!(sided[0], Printer { name: "Office PCL".into(), default: true, two_sided: Some(true) });
    assert_eq!(sided[1], Printer { name: "Office PS".into(), default: false, two_sided: Some(false) });
    assert_eq!(sided[2], Printer { name: "Tab\tName".into(), default: false, two_sided: Some(true) });
}

#[test]
fn the_windows_job_is_the_environment_of_the_print_script() {
    let path = std::path::Path::new("C:\\Users\\me\\AppData\\Local\\Temp\\craft-print-42.pdf");
    let job = Job {
        printer: Some("HP \"Laser\" Jet".into()),
        copies: 3,
        duplex: Some(Duplex::ShortEdge),
        grayscale: true,
        title: "Angebot.pdf".into(),
        ..Job::default()
    };
    let vars = windows_env_vars(path, &job);
    let env = |key: &str| vars.iter().find(|(k, _)| k == key).map(|(_, v)| v.as_str()).unwrap();
    assert_eq!(env("CRAFT_PRINT_FILE"), "C:\\Users\\me\\AppData\\Local\\Temp\\craft-print-42.pdf");
    // Quotes and spaces in a printer name survive because nothing goes on the command line.
    assert_eq!(env("CRAFT_PRINT_PRINTER"), "HP \"Laser\" Jet");
    assert_eq!(env("CRAFT_PRINT_COPIES"), "3");
    assert_eq!(env("CRAFT_PRINT_COLLATE"), "1");
    assert_eq!(env("CRAFT_PRINT_DUPLEX"), "short-edge");
    assert_eq!(env("CRAFT_PRINT_SCALING"), "exact");
    assert_eq!(env("CRAFT_PRINT_GRAYSCALE"), "1");
    assert_eq!(env("CRAFT_PRINT_TITLE"), "Angebot.pdf");
    // No printer chosen means the system default: an empty name, not the word "default".
    let one = |j: &Job, key: &str| windows_env_vars(path, j).into_iter().find(|(k, _)| k == key).unwrap().1;
    assert_eq!(one(&Job::default(), "CRAFT_PRINT_PRINTER"), "");
    assert_eq!(one(&Job::default(), "CRAFT_PRINT_DUPLEX"), "");
    assert_eq!(one(&Job { scaling: Scaling::ShrinkToFit, ..Job::default() }, "CRAFT_PRINT_SCALING"), "shrink-to-fit");
    assert_eq!(one(&Job { duplex: Some(Duplex::Off), ..Job::default() }, "CRAFT_PRINT_DUPLEX"), "off");
    assert_eq!(one(&Job { duplex: Some(Duplex::LongEdge), ..Job::default() }, "CRAFT_PRINT_DUPLEX"), "long-edge");
}

/// Run a PowerShell script for the Windows-only tests: its stdout, or everything it said when
/// it failed — the tests print that as the skip reason.
#[cfg(windows)]
fn windows_powershell(script: &str, envs: &[(&str, &str)]) -> Result<String, String> {
    let mut c = std::process::Command::new("powershell");
    c.args(["-NoProfile", "-NonInteractive", "-Command", &format!("$ErrorActionPreference = 'Stop'; {script}")]);
    for (k, v) in envs {
        c.env(k, v);
    }
    match c.output() {
        Ok(o) if o.status.success() => Ok(String::from_utf8_lossy(&o.stdout).into_owned()),
        Ok(o) => Err(format!("{} {}", String::from_utf8_lossy(&o.stderr).trim(), String::from_utf8_lossy(&o.stdout).trim())),
        Err(e) => Err(e.to_string()),
    }
}

/// Windows: the printer list comes from the spooler. A bare machine may have no printer; the
/// list that exists has real names and at most one default.
#[cfg(windows)]
#[test]
fn windows_lists_the_printers_the_system_knows() {
    let printers = spool::printers();
    assert!(printers.iter().all(|p| !p.name.trim().is_empty() && p.two_sided.is_some()), "{printers:?}");
    assert!(printers.iter().filter(|p| p.default).count() <= 1, "{printers:?}");
}

/// Windows: the embedded print script is valid PowerShell, checked on every machine — including
/// runners that have no printer to print to.
#[cfg(windows)]
#[test]
fn the_windows_print_script_parses() {
    let path = std::env::temp_dir().join(format!("craft-print-script-{}.ps1", std::process::id()));
    std::fs::write(&path, spool::WINDOWS_PRINT_SCRIPT).unwrap();
    let errors = windows_powershell(
        "$tokens = $null; $errors = $null; [System.Management.Automation.Language.Parser]::ParseFile($env:CRAFT_TEST_SCRIPT, [ref]$tokens, [ref]$errors) > $null; $errors.Count",
        &[("CRAFT_TEST_SCRIPT", path.to_str().unwrap())],
    );
    let _ = std::fs::remove_file(&path);
    assert_eq!(errors.as_deref().map(str::trim), Ok("0"), "parse errors: {errors:?}");
}

/// Windows, end to end: the script renders the job's sheets with Windows.Data.Pdf on any
/// machine — CI included, no printer needed (the dry run renders sheets in memory) — and, where a paused test printer can be added, a real job lands in its queue.
/// Its driver is the inbox "Generic / Text Only"; a machine that cannot add one (no rights, no
/// driver) skips that half, as the CUPS tests skip where CUPS isn't running.
#[cfg(windows)]
#[test]
fn the_print_script_renders_the_job_and_reaches_the_spooler() {
    let pdf = fixture();
    // The printer name is deliberately wrong: the dry run stops before the driver is touched.
    let job = Job { printer: Some("no such printer".into()), title: "test.pdf".into(), ..Job::default() };
    let dry = spool::windows_submit(&pdf, &job, &[("CRAFT_PRINT_DRYRUN", "1")]);
    assert_eq!(dry.as_deref(), Ok("rendered 2 sheet(s)"), "{dry:?}");

    let name = format!("Craft Test Print {}", std::process::id());
    let name = name.as_str();
    let added = windows_powershell(
        r#"
        $name = $env:CRAFT_TEST_PRINTER
        $port = $name + '.prn'
        if (Get-Printer -Name $name -ErrorAction SilentlyContinue) { throw 'test queue already exists' }
        if ((Get-PrinterPort -Name $port -ErrorAction SilentlyContinue) -or (Test-Path -LiteralPath $port)) {
            throw 'test port already exists'
        }
        $portCreated = $false
        try {
            # FILE: can ask GDI for an output filename even for a paused queue. A uniquely named
            # local port has no prompt; the paused job is removed before it can write.
            Add-PrinterPort -Name $port
            $portCreated = $true
            Add-Printer -Name $name -DriverName 'Generic / Text Only' -PortName $port
            $queue = Get-CimInstance Win32_Printer | Where-Object Name -eq $name
            if (-not $queue) { throw 'test queue could not be found' }
            $paused = Invoke-CimMethod -InputObject $queue -MethodName Pause
            if ($paused.ReturnValue -ne 0) { throw 'test queue could not be paused' }
        } catch {
            Remove-Printer -Name $name -ErrorAction SilentlyContinue
            if ($portCreated) { Remove-PrinterPort -Name $port -ComputerName $env:COMPUTERNAME -ErrorAction SilentlyContinue }
            throw
        }
        "#,
        &[("CRAFT_TEST_PRINTER", name)],
    );
    if let Err(why) = added {
        assert_ne!(std::env::var("CRAFT_PRINT_REQUIRE_QUEUE").as_deref(), Ok("1"), "CI requires a prepared paused printer: {why}");
        eprintln!("skipping the real print: the test printer could not be prepared: {why}");
        return;
    }
    let job = Job { printer: Some(name.into()), title: "test.pdf".into(), ..Job::default() };
    let sent = spool::submit(&pdf, &job);
    let queued = windows_powershell(
        &format!("if (Get-PrintJob -PrinterName '{name}') {{ (Get-PrintJob -PrinterName '{name}' | Measure-Object).Count }} else {{ 0 }}"),
        &[],
    );
    let cleaned = windows_powershell(
        r#"
        $name = $env:CRAFT_TEST_PRINTER
        $port = $name + '.prn'
        try { Get-PrintJob -PrinterName $name | Remove-PrintJob } finally {
            try { Remove-Printer -Name $name } finally {
                Remove-PrinterPort -Name $port -ComputerName $env:COMPUTERNAME
                if (Test-Path -LiteralPath $port) { Remove-Item -LiteralPath $port }
            }
        }
        if (Get-Printer -Name $name -ErrorAction SilentlyContinue) { throw 'test queue was not removed' }
        if ((Get-PrinterPort -Name $port -ErrorAction SilentlyContinue) -or (Test-Path -LiteralPath $port)) {
            throw 'test port was not removed'
        }
        Write-Output 'cleaned'
        "#,
        &[("CRAFT_TEST_PRINTER", name)],
    );
    assert!(sent.is_ok(), "{sent:?}");
    assert_eq!(queued.as_deref().map(str::trim), Ok("1"), "the job is in the paused printer's queue: {queued:?}");
    assert!(cleaned.is_ok(), "the test printer is removed again: {cleaned:?}");
}

/// A PPD shaped like a Fiery's: installable options, multi-line PostScript in the choices, Latin-1
/// labels, options the Print dialog sets itself.
const FIERY_PPD: &[u8] = b"*PPD-Adobe: \"4.3\"\n\
*LanguageEncoding: ISOLatin1\n\
*OpenGroup: InstallableOptions/Installable Options\n\
*OpenUI *EFFinisher/Finisher option: PickOne\n\
*DefaultEFFinisher: False\n\
*EFFinisher False/Not installed: \"\"\n\
*EFFinisher SingleStapler/Single stapler: \"\"\n\
*CloseUI: *EFFinisher\n\
*CloseGroup: InstallableOptions\n\
*OpenGroup: FPPaperSource/Media\n\
*OpenUI *InputSlot/Paper tray: PickOne\n\
*OrderDependency: 20.0 AnySetup *InputSlot\n\
*DefaultInputSlot: AutoSelect\n\
*InputSlot AutoSelect/Auto tray select: \"\n\
userdict /XJXEFIsetpageproperties known\n\
{ << /XJXsettrayselV2 [ 7 ] >> XJXEFIsetpageproperties } if\"\n\
*End\n\
*InputSlot ManualFeed/Bypass tray: \"\n\
{ pop 2 XJXsettrayselV2 } if\"\n\
*End\n\
*InputSlot Tray2/Tray 2: \"\"\n\
*fr.InputSlot Tray2/Bac 2: \"\"\n\
*CloseUI: *InputSlot\n\
*OpenUI *EFMediaType/Paper type: PickOne\n\
*DefaultEFMediaType: Plain\n\
*EFMediaType Plain/Plain: \"\"\n\
*EFMediaType Heavy1/Thick 1 (106\xad163 g/m\xb2): \"\"\n\
*CloseUI: *EFMediaType\n\
*CloseGroup: FPPaperSource\n\
*OpenUI *PageSize/Page size: PickOne\n\
*DefaultPageSize: A4\n\
*PageSize A4/A4: \"\"\n\
*CloseUI: *PageSize\n\
*OpenUI *EFRaster/Print queue action: PickOne\n\
*DefaultEFRaster: Bogus\n\
*EFRaster False/Print: \"\"\n\
*EFRaster Hold: \"\"\n\
*CloseUI: *EFRaster\n";

#[test]
fn printer_options_come_from_the_ppd() {
    let options = spool::parse_ppd(&spool::ppd_text(FIERY_PPD));
    let keys: Vec<&str> = options.iter().map(|o| o.key.as_str()).collect();
    assert_eq!(keys, ["InputSlot", "EFMediaType", "EFRaster"], "no installable options, no PageSize");
    let tray = &options[0];
    assert_eq!((tray.label.as_str(), tray.group.as_str(), tray.default.as_str()), ("Paper tray", "Media", "AutoSelect"));
    assert_eq!(
        tray.choices,
        [("AutoSelect".into(), "Auto tray select".into()), ("ManualFeed".into(), "Bypass tray".into()), ("Tray2".into(), "Tray 2".into())],
        "PostScript lines and translations (*fr.InputSlot) are not choices"
    );
    assert_eq!(options[1].choices[1].1, "Thick 1 (106\u{ad}163 g/m²)", "Latin-1 labels");
    // A default that isn't one of the choices falls back to the first; a choice without a label shows its keyword.
    assert_eq!((options[2].default.as_str(), options[2].choices[1].1.as_str()), ("False", "Hold"));
    assert!(spool::parse_ppd("*OpenUI *Broken\n*Broken A/B\n").is_empty(), "unclosed or malformed blocks are skipped");
}

#[test]
fn printer_option_defaults_and_job_arguments() {
    let current =
        spool::parse_lpoptions("InputSlot/Paper tray: AutoSelect *Tray2 ManualFeed\nEFRaster/Print queue action: *False Hold\nbroken line\n");
    assert_eq!(current, [("InputSlot".to_string(), "Tray2".to_string()), ("EFRaster".to_string(), "False".to_string())]);
    let job = Job {
        options: vec![
            ("InputSlot".into(), "Tray2".into()),
            ("EFMediaType".into(), "Heavy1".into()),
            ("Duplex".into(), "DuplexTumble".into()),
            ("Bad Key".into(), "x".into()),
            ("EFRaster".into(), "a=b".into()),
        ],
        ..Job::default()
    };
    let args = lp_args(&job).join(" ");
    assert!(args.ends_with("-o fit-to-page=false -o InputSlot=Tray2 -o EFMediaType=Heavy1"), "{args}");
    assert!(spool::printer_options("../../etc/passwd").is_empty(), "not a queue name");
}

#[cfg(windows)]
fn fixture() -> Vec<u8> {
    // Minimal, asset-free PDF with portrait and landscape Letter sheets.
    let objects = [
        "<< /Type /Catalog /Pages 2 0 R >>",
        "<< /Type /Pages /Kids [3 0 R 4 0 R] /Count 2 >>",
        "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] /Resources << >> /Contents 5 0 R >>",
        "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 792 612] /Resources << >> /Contents 5 0 R >>",
        "<< /Length 26 >>\nstream\n0 0 0 rg 36 36 72 72 re f\nendstream",
    ];
    let mut pdf = String::from("%PDF-1.4\n");
    let mut offsets = vec![0];
    for (i, object) in objects.iter().enumerate() {
        offsets.push(pdf.len());
        pdf.push_str(&format!("{} 0 obj\n{object}\nendobj\n", i + 1));
    }
    let xref = pdf.len();
    pdf.push_str(&format!("xref\n0 {}\n0000000000 65535 f \n", offsets.len()));
    for offset in offsets.iter().skip(1) {
        pdf.push_str(&format!("{offset:010} 00000 n \n"));
    }
    pdf.push_str(&format!("trailer\n<< /Size {} /Root 1 0 R >>\nstartxref\n{xref}\n%%EOF\n", offsets.len()));
    pdf.into_bytes()
}

#[cfg(windows)]
#[test]
fn windows_preserves_sheet_size_orientation_and_binding_edge() {
    let output = windows_powershell(WINDOWS_PRINT_SCRIPT, &[("CRAFT_PRINT_TEST_SETTINGS", "1")]).unwrap();
    let r: serde_json::Value = serde_json::from_str(output.trim()).unwrap();
    assert_eq!(r["LongEdge"], "Vertical");
    assert_eq!(r["ShortEdge"], "Horizontal");
    assert_eq!(r["Off"], "Simplex");
    assert_eq!(r["DefaultDuplex"], "Vertical");
    assert_eq!(r["ForcedSimplex"], "Simplex");
    assert_eq!(r["FallbackPaper"], "Driver Letter");
    // Windows PowerShell serializes whole-valued floats as JSON integers. Compare their
    // numeric values rather than requiring one particular JSON number representation.
    let boxes = r["ShrinkBoxes"].as_array().unwrap();
    let expected = [[0.0, 20.0, 800.0, 1000.0], [200.0, 270.0, 400.0, 500.0]];
    assert_eq!(boxes.len(), expected.len());
    for (actual, expected) in boxes.iter().zip(expected) {
        let actual = actual.as_array().unwrap();
        assert_eq!(actual.len(), expected.len());
        for (actual, expected) in actual.iter().zip(expected) {
            assert_eq!(actual.as_f64(), Some(expected));
        }
    }
    let sheets = r["Sheets"].as_array().unwrap();
    assert_eq!((sheets[0]["Width"].as_u64(), sheets[0]["Height"].as_u64()), (Some(850), Some(1100)));
    assert_eq!(sheets[0]["Name"], "Driver Letter");
    assert_eq!(sheets[0]["Pixels"], serde_json::json!([1275, 1650]));
    assert_eq!(sheets[1]["Landscape"], true);
    assert_eq!(sheets[1]["Pixels"], serde_json::json!([1650, 1275]));
    assert_eq!((sheets[2]["Width"].as_u64(), sheets[2]["Height"].as_u64()), (Some(827), Some(1169)));
    assert_eq!(sheets[3]["Height"], 1400);
    for s in sheets {
        assert_eq!(s["Margins"], serde_json::json!([0, 0, 0, 0]));
        assert_eq!(s["X"].as_f64(), Some(-25.0));
        assert_eq!(s["Y"].as_f64(), Some(-30.0));
    }
    assert_eq!(sheets[0]["DrawWidth"].as_f64(), Some(850.0));
    assert_eq!(sheets[1]["DrawWidth"].as_f64(), Some(1100.0));
    let pixels = r["LargePixels"].as_array().unwrap();
    assert!(pixels[0].as_u64().unwrap() * pixels[1].as_u64().unwrap() <= 64_000_000);
    assert_eq!(r["InvalidRejected"], true);
}

#[cfg(windows)]
#[test]
fn windows_job_files_are_distinct_and_removed_on_drop() {
    let first = windows_job_file(b"first").unwrap();
    let second = windows_job_file(b"second").unwrap();
    assert_ne!(first.to_path_buf(), second.to_path_buf());
    assert_eq!(std::fs::read(&first).unwrap(), b"first");
    let path = first.to_path_buf();
    drop(first);
    assert!(!path.exists());
}

#[test]
fn command_previews_do_not_require_pdf_files() {
    let job = Job { printer: Some("Office (east)".into()), copies: 2, ..Job::default() };
    let preview = command_preview(&job).unwrap();
    if backend() == Some(Backend::Cups) {
        assert_eq!(preview[0], "lp");
        assert_eq!(preview[1..], lp_args(&job));
    } else {
        assert_eq!(preview[0], "powershell");
        assert!(!preview.join(" ").contains(".pdf"));
    }
}

#[test]
fn queue_duplex_defaults_and_explicit_scaling_reach_the_spooler() {
    let default_args = lp_args(&Job::default());
    assert!(!default_args.iter().any(|a| a.starts_with("sides=")));
    let simplex = lp_args(&Job { duplex: Some(Duplex::Off), ..Job::default() });
    assert!(simplex.iter().any(|a| a == "sides=one-sided"));
    let shrink = lp_args(&Job { scaling: Scaling::ShrinkToFit, ..Job::default() });
    assert!(shrink.iter().any(|a| a == "print-scaling=auto-fit"));
    assert!(!shrink.iter().any(|a| a.starts_with("fit-to-page=")));
}

/// A portable app must never resolve PowerShell from its own directory.
#[cfg(windows)]
#[test]
fn windows_starts_powershell_by_its_full_path() {
    let command = powershell_command("$null");
    let program = std::path::Path::new(command.get_program());
    assert!(program.is_absolute(), "{program:?}");
    assert!(program.ends_with("System32/WindowsPowerShell/v1.0/powershell.exe"), "{program:?}");
    assert!(program.is_file(), "{program:?}");
}
