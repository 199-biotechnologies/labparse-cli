use std::process::Command;

fn labparse() -> Command {
    Command::new(env!("CARGO_BIN_EXE_labparse"))
}

#[test]
fn test_text_flag_json() {
    let output = labparse()
        .args(["--text", "HbA1c 5.8%, ApoB 95 mg/dL, LDL 130 mg/dL", "--json"])
        .output()
        .expect("failed to run labparse");

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    let json: serde_json::Value = serde_json::from_str(&stdout).expect("invalid JSON");

    assert_eq!(json["version"], "1");
    assert_eq!(json["status"], "success");
    assert!(json["data"]["biomarkers"].as_array().unwrap().len() >= 2);
    assert!(json["metadata"]["markers_found"].as_u64().unwrap() >= 2);
}

#[test]
fn test_text_single_marker() {
    let output = labparse()
        .args(["--text", "Fasting Glucose 92 mg/dL", "--json"])
        .output()
        .expect("failed to run labparse");

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    let json: serde_json::Value = serde_json::from_str(&stdout).expect("invalid JSON");

    let bms = json["data"]["biomarkers"].as_array().unwrap();
    assert_eq!(bms.len(), 1);
    assert_eq!(bms[0]["standardized_name"], "fasting_glucose");
    assert_eq!(bms[0]["value"], 92.0);
}

#[test]
fn test_agent_info() {
    let output = labparse()
        .arg("agent-info")
        .output()
        .expect("failed to run labparse");

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    let json: serde_json::Value = serde_json::from_str(&stdout).expect("invalid JSON");

    assert_eq!(json["name"], "labparse");
    assert!(json["biomarker_count"].as_u64().unwrap() > 100);
}

#[test]
fn test_no_input_exits_with_code_2() {
    let output = labparse()
        .output()
        .expect("failed to run labparse");

    assert!(!output.status.success());
    // Exit code 2 = config/usage error
    assert_eq!(output.status.code(), Some(2));
}

#[test]
fn test_csv_parsing() {
    let csv_content = "Test Name,Result,Units\nHbA1c,5.8,%\nLDL Cholesterol,130,mg/dL\nApoB,95,mg/dL\n";
    let tmp = std::env::temp_dir().join("labparse_test.csv");
    std::fs::write(&tmp, csv_content).unwrap();

    let output = labparse()
        .args([tmp.to_str().unwrap(), "--json"])
        .output()
        .expect("failed to run labparse");

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    let json: serde_json::Value = serde_json::from_str(&stdout).expect("invalid JSON");

    assert_eq!(json["metadata"]["parser"], "csv");
    let bms = json["data"]["biomarkers"].as_array().unwrap();
    assert!(bms.len() >= 2);

    std::fs::remove_file(&tmp).ok();
}

#[test]
fn test_stdin_parsing() {
    use std::io::Write;
    use std::process::Stdio;

    let mut child = labparse()
        .args(["--stdin", "--json"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("failed to spawn labparse");

    let stdin = child.stdin.as_mut().unwrap();
    stdin
        .write_all(b"HbA1c 5.8%, Triglycerides 150 mg/dL")
        .unwrap();
    drop(child.stdin.take());

    let output = child.wait_with_output().expect("failed to wait");
    assert!(output.status.success());

    let stdout = String::from_utf8_lossy(&output.stdout);
    let json: serde_json::Value = serde_json::from_str(&stdout).expect("invalid JSON");
    assert!(json["data"]["biomarkers"].as_array().unwrap().len() >= 1);
}

#[test]
fn test_biomarkers_subcommand() {
    let output = labparse()
        .args(["biomarkers", "--json"])
        .output()
        .expect("failed to run labparse");

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    let json: serde_json::Value = serde_json::from_str(&stdout).expect("invalid JSON");

    assert!(json.as_array().unwrap().len() > 100);
}

#[test]
fn test_file_not_found_exits_with_code_2() {
    let output = labparse()
        .arg("/nonexistent/file.csv")
        .output()
        .expect("failed to run labparse");

    assert!(!output.status.success());
    assert_eq!(output.status.code(), Some(2));
}

#[test]
fn test_multiple_text_markers() {
    let output = labparse()
        .args([
            "--text",
            "HbA1c 5.8%, ApoB 95 mg/dL, LDL 130 mg/dL, Fasting Glucose 92 mg/dL, Triglycerides 150 mg/dL, HDL 55 mg/dL",
            "--json",
        ])
        .output()
        .expect("failed to run labparse");

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    let json: serde_json::Value = serde_json::from_str(&stdout).expect("invalid JSON");

    let bms = json["data"]["biomarkers"].as_array().unwrap();
    assert!(bms.len() >= 4, "Expected at least 4 biomarkers, got {}", bms.len());
}

#[test]
fn test_decimal_comma_text() {
    let output = labparse()
        .args(["--text", "HbA1c 5,8%", "--json"])
        .output()
        .expect("failed to run labparse");

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    let json: serde_json::Value = serde_json::from_str(&stdout).expect("invalid JSON");

    let bms = json["data"]["biomarkers"].as_array().unwrap();
    assert_eq!(bms.len(), 1);
    assert_eq!(bms[0]["standardized_name"], "hba1c");
    assert!((bms[0]["value"].as_f64().unwrap() - 5.8).abs() < 0.001);
}

#[test]
fn test_bom_csv() {
    let csv_content = "\u{FEFF}Test Name,Result,Units\nHbA1c,5.8,%\nLDL Cholesterol,130,mg/dL\n";
    let tmp = std::env::temp_dir().join("labparse_test_bom.csv");
    std::fs::write(&tmp, csv_content).unwrap();

    let output = labparse()
        .args([tmp.to_str().unwrap(), "--json"])
        .output()
        .expect("failed to run labparse");

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    let json: serde_json::Value = serde_json::from_str(&stdout).expect("invalid JSON");

    assert_eq!(json["metadata"]["parser"], "csv");
    let bms = json["data"]["biomarkers"].as_array().unwrap();
    assert!(bms.len() >= 2, "Expected at least 2 biomarkers from BOM CSV, got {}", bms.len());

    std::fs::remove_file(&tmp).ok();
}

#[test]
fn test_thyroglobulin_alias() {
    let output = labparse()
        .args(["--text", "Thyroglobulin Antibodies (TG Abs) 45 IU/mL", "--json"])
        .output()
        .expect("failed to run labparse");

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    let json: serde_json::Value = serde_json::from_str(&stdout).expect("invalid JSON");

    let bms = json["data"]["biomarkers"].as_array().unwrap();
    assert_eq!(bms.len(), 1, "Expected 1 biomarker, got {}", bms.len());
    assert_eq!(bms[0]["standardized_name"], "anti_tg");
}

#[test]
fn test_alias_determinism() {
    // Call resolve_name many times and verify consistent output
    let output = labparse()
        .args(["--text", "HbA1c 5.8%, LDL 130 mg/dL, ApoB 95 mg/dL", "--json"])
        .output()
        .expect("failed to run labparse");

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    let first: serde_json::Value = serde_json::from_str(&stdout).expect("invalid JSON");

    // Run the same parse 10 more times and verify identical output
    for _ in 0..10 {
        let output2 = labparse()
            .args(["--text", "HbA1c 5.8%, LDL 130 mg/dL, ApoB 95 mg/dL", "--json"])
            .output()
            .expect("failed to run labparse");

        let stdout2 = String::from_utf8_lossy(&output2.stdout);
        let second: serde_json::Value = serde_json::from_str(&stdout2).expect("invalid JSON");

        assert_eq!(
            first["data"]["biomarkers"], second["data"]["biomarkers"],
            "Alias resolution was non-deterministic"
        );
    }
}

#[test]
fn test_semicolon_delimited_csv() {
    let csv_content = "Test Name;Result;Units\nHbA1c;5.8;%\nLDL Cholesterol;130;mg/dL\n";
    let tmp = std::env::temp_dir().join("labparse_test_semi.csv");
    std::fs::write(&tmp, csv_content).unwrap();

    let output = labparse()
        .args([tmp.to_str().unwrap(), "--json"])
        .output()
        .expect("failed to run labparse");

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    let json: serde_json::Value = serde_json::from_str(&stdout).expect("invalid JSON");

    assert_eq!(json["metadata"]["parser"], "csv");
    let bms = json["data"]["biomarkers"].as_array().unwrap();
    assert!(bms.len() >= 2, "Expected at least 2 biomarkers from semicolon CSV, got {}", bms.len());

    std::fs::remove_file(&tmp).ok();
}

#[test]
fn test_tab_delimited_csv() {
    let csv_content = "Test Name\tResult\tUnits\nHbA1c\t5.8\t%\nLDL Cholesterol\t130\tmg/dL\n";
    let tmp = std::env::temp_dir().join("labparse_test_tab.csv");
    std::fs::write(&tmp, csv_content).unwrap();

    let output = labparse()
        .args([tmp.to_str().unwrap(), "--json"])
        .output()
        .expect("failed to run labparse");

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    let json: serde_json::Value = serde_json::from_str(&stdout).expect("invalid JSON");

    assert_eq!(json["metadata"]["parser"], "csv");
    let bms = json["data"]["biomarkers"].as_array().unwrap();
    assert!(bms.len() >= 2, "Expected at least 2 biomarkers from tab CSV, got {}", bms.len());

    std::fs::remove_file(&tmp).ok();
}

#[test]
fn test_space_separated_markers() {
    let output = labparse()
        .args([
            "--text",
            "HbA1c 5.8% ApoB 95 mg/dL LDL 130 mg/dL",
            "--json",
        ])
        .output()
        .expect("failed to run labparse");

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    let json: serde_json::Value = serde_json::from_str(&stdout).expect("invalid JSON");

    let bms = json["data"]["biomarkers"].as_array().unwrap();
    assert_eq!(bms.len(), 3, "Expected 3 biomarkers, got {}", bms.len());
    assert_eq!(bms[0]["standardized_name"], "hba1c");
    assert_eq!(bms[1]["standardized_name"], "apolipoprotein_b");
    assert_eq!(bms[2]["standardized_name"], "ldl_cholesterol");
}
