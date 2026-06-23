use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use super::types::EvalReport;

pub(super) fn write_report(report: &EvalReport) -> Result<PathBuf, String> {
    let results_dir = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("evaluation")
        .join("results");
    fs::create_dir_all(&results_dir)
        .map_err(|err| format!("failed to create {}: {err}", results_dir.display()))?;

    let report_file = results_dir.join(format!(
        "{}.json",
        report.timestamp.replace([':', '.'], "-")
    ));
    let json = serde_json::to_string_pretty(report)
        .map_err(|err| format!("failed to serialize eval report: {err}"))?;
    fs::write(&report_file, json)
        .map_err(|err| format!("failed to write {}: {err}", report_file.display()))?;
    Ok(report_file)
}

pub(super) fn iso_timestamp(time: SystemTime) -> String {
    let duration = time
        .duration_since(UNIX_EPOCH)
        .expect("system time should be after Unix epoch");
    let total_seconds = duration.as_secs() as i64;
    let millis = duration.subsec_millis();
    let days = total_seconds.div_euclid(86_400);
    let seconds_of_day = total_seconds.rem_euclid(86_400);
    let (year, month, day) = civil_from_days(days);
    let hour = seconds_of_day / 3_600;
    let minute = (seconds_of_day % 3_600) / 60;
    let second = seconds_of_day % 60;

    format!("{year:04}-{month:02}-{day:02}T{hour:02}:{minute:02}:{second:02}.{millis:03}Z")
}

fn civil_from_days(days_since_unix_epoch: i64) -> (i64, u32, u32) {
    let z = days_since_unix_epoch + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = mp + if mp < 10 { 3 } else { -9 };
    let year = y + i64::from(month <= 2);

    (year, month as u32, day as u32)
}
