use super::DownloadProgress;
use std::path::Path;

/// Observe the actual curl output file, including resets during automatic retries.
pub(super) struct Reporter<'a> {
    package: &'a Path,
    headers: &'a Path,
}
impl<'a> Reporter<'a> {
    pub fn new(package: &'a Path, headers: &'a Path) -> Self {
        Self { package, headers }
    }
    pub fn sample(&mut self) -> DownloadProgress {
        DownloadProgress {
            downloaded_bytes: std::fs::metadata(self.package)
                .map(|m| m.len())
                .unwrap_or(0),
            total_bytes: std::fs::read_to_string(self.headers)
                .ok()
                .and_then(|h| content_length(&h)),
        }
    }
}

// Only the final successful response describes this package. Redirects, proxy
// CONNECT replies and earlier retries may have unrelated Content-Length values.
fn content_length(headers: &str) -> Option<u64> {
    let response = headers.rsplit("HTTP/").next()?;
    let (head, _) = response.split_once("\r\n\r\n")?;
    let mut lines = head.lines();
    let code: u16 = lines.next()?.split_whitespace().nth(1)?.parse().ok()?;
    if !(200..300).contains(&code) {
        return None;
    }
    let mut length = None;
    for line in lines {
        if let Some((name, value)) = line.split_once(':') {
            if name.eq_ignore_ascii_case("transfer-encoding") {
                return None;
            }
            if name.eq_ignore_ascii_case("content-length") {
                length = value.trim().parse::<u64>().ok().filter(|n| *n > 0);
            }
        }
    }
    length
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn progress_tracks_the_actual_file_and_resets_when_curl_retries() {
        let root = tempfile::tempdir().unwrap();
        let package = root.path().join("package");
        let headers = root.path().join("headers");
        let mut reporter = Reporter::new(&package, &headers);
        assert_eq!(reporter.sample().downloaded_bytes, 0);
        std::fs::write(&headers, "HTTP/2 200 OK\r\nContent-Length: 100\r\n\r\n").unwrap();
        std::fs::write(&package, [0; 25]).unwrap();
        assert_eq!(
            reporter.sample(),
            DownloadProgress {
                downloaded_bytes: 25,
                total_bytes: Some(100)
            }
        );
        std::fs::write(&package, [0; 60]).unwrap();
        assert_eq!(reporter.sample().downloaded_bytes, 60);
        std::fs::write(&package, []).unwrap();
        std::fs::write(&headers, "HTTP/2 200 OK\r\n").unwrap();
        assert_eq!(
            reporter.sample(),
            DownloadProgress {
                downloaded_bytes: 0,
                total_bytes: None
            }
        );
    }
    #[test]
    fn final_response_length_handles_redirects_retries_and_unknown_sizes() {
        let redirect = "HTTP/1.1 302 Found\r\nContent-Length: 50\r\n\r\n";
        assert_eq!(
            content_length(&format!(
                "{redirect}HTTP/2 200 OK\r\ncontent-length: 104857600\r\n\r\n"
            )),
            Some(104857600)
        );
        assert_eq!(
            content_length(&format!("{redirect}HTTP/2 200 OK\r\n")),
            None
        );
        assert_eq!(
            content_length("HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n"),
            None
        );
        assert_eq!(
            content_length("HTTP/1.1 503 Busy\r\nContent-Length: 80\r\n\r\n"),
            None
        );
    }
}
