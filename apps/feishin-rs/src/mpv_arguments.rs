//! Explicit audio/cache options, one process argument per line. Never a shell.
const INVALID: &str = "MPV parameters must be audio or cache options, one --name=value argument per line (at most 64 lines and 16 KiB). Scripts, files, configuration and IPC overrides are not supported.";

pub(super) fn parse(input: &str) -> Result<Vec<String>, &'static str> {
    if input.len() > 16 * 1024 {
        return Err(INVALID);
    }
    let mut arguments = Vec::new();
    for line in input.lines().map(str::trim).filter(|line| !line.is_empty()) {
        if arguments.len() >= 64 || line.len() > 4096 || line.chars().any(char::is_control) {
            return Err(INVALID);
        }
        let option = line.strip_prefix("--").ok_or(INVALID)?;
        let name = option.split_once('=').map_or(option, |(name, _)| name);
        let name = name.strip_prefix("no-").unwrap_or(name);
        if !matches!(
            name,
            "ao" | "ad"
                | "audio-buffer"
                | "audio-client-name"
                | "audio-spdif"
                | "audio-resample-filter-size"
                | "audio-resample-phase-shift"
                | "audio-resample-cutoff"
                | "audio-resample-linear"
                | "audio-resample-max-size"
                | "audio-stream-silence"
                | "audio-wait-open"
                | "cache"
                | "cache-pause"
                | "cache-pause-initial"
                | "cache-pause-wait"
                | "cache-secs"
                | "demuxer-max-bytes"
                | "demuxer-max-back-bytes"
                | "demuxer-readahead-secs"
        ) {
            return Err(INVALID);
        }
        arguments.push(line.to_owned());
    }
    Ok(arguments)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn audio_arguments_are_literal_and_line_separated_without_shell_parsing() {
        assert_eq!(
            parse(" --ao=null \n\n--audio-client-name=Music Player\r\n--cache-secs=20").unwrap(),
            [
                "--ao=null",
                "--audio-client-name=Music Player",
                "--cache-secs=20"
            ]
        );
        // Metacharacters inside a value are not executable: each line remains a
        // single std::process argument. No shell, quotes or expansion is used.
        assert_eq!(
            parse("--audio-client-name=$(do-not-run)").unwrap(),
            ["--audio-client-name=$(do-not-run)"]
        );
    }
    #[test]
    fn files_scripts_ipc_and_unbounded_options_are_rejected_without_echoing_values() {
        for input in [
            "/tmp/music.flac",
            "--",
            "--script=secret.lua",
            "--config=yes",
            "--input-ipc-server=/tmp/secret",
            "--log-file=secret",
            "--audio-device=bad\0value",
        ] {
            assert_eq!(parse(input), Err(INVALID));
            assert!(!parse(input).unwrap_err().contains("secret"));
        }
        assert!(parse(&"--cache=yes\n".repeat(65)).is_err());
        assert!(parse(&format!("--audio-client-name={}", "x".repeat(17000))).is_err());
    }
}
