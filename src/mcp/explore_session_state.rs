//! Bounded, connection-owned accounting of source actually returned by explore.
use std::collections::VecDeque;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileEmission {
    pub path: String,
    pub ranges: Vec<(usize, usize)>,
    pub fingerprint: [u8; 32],
}
#[derive(Debug, Clone)]
pub struct CallRecord {
    pub files: Vec<FileEmission>,
    pub response_bytes: usize,
}
#[derive(Debug, Clone)]
pub struct ProjectState {
    pub root: PathBuf,
    pub call_count: usize,
    pub response_bytes: usize,
    pub calls: VecDeque<CallRecord>,
}
#[derive(Debug, Clone, Default)]
pub struct ExploreSessionState {
    pub projects: VecDeque<ProjectState>,
}
impl ExploreSessionState {
    /// Replace only verified, substantial contiguous runs from this connection.
    pub fn deduplicate(&self, root: &Path, response: &str) -> String {
        let mut current = Self::default();
        current.record_response(root, response);
        let Some(now) = current.projects.front() else {
            return response.to_owned();
        };
        let Some(previous) = self.projects.iter().find(|p| p.root == now.root) else {
            return response.to_owned();
        };
        let mut output = response.to_owned();
        for file in &now.calls[0].files {
            for &(start, end) in &file.ranges {
                if end - start + 1 < 8 {
                    continue;
                }
                let covered = previous.calls.iter().any(|call| {
                    call.files.iter().any(|old| {
                        old.path == file.path
                            && old.fingerprint == file.fingerprint
                            && old.ranges.iter().any(|&(a, b)| a <= start && b >= end)
                    })
                });
                if !covered {
                    continue;
                }
                let Ok(source) = std::fs::read_to_string(now.root.join(&file.path)) else {
                    continue;
                };
                let block = source
                    .lines()
                    .enumerate()
                    .filter(|(i, _)| i + 1 >= start && i + 1 <= end)
                    .map(|(i, line)| format!("{}\t{}\n", i + 1, line))
                    .collect::<String>();
                // Scope replacement to this file section, never another file's equal lines.
                let header = format!("#### {} · ", file.path);
                if let Some(begin) = output.find(&header) {
                    let stop = output[begin + header.len()..]
                        .find("\n#### ")
                        .map(|i| begin + header.len() + i)
                        .unwrap_or(output.len());
                    if let Some(offset) = output[begin..stop].find(&block) {
                        output.replace_range(begin+offset..begin+offset+block.len(), &format!("Source already sent in this session: {}:{}-{}; file unchanged; treat as already Read.\n", file.path,start,end));
                    }
                }
            }
        }
        output
    }
    pub fn record_response(&mut self, root: &Path, response: &str) {
        use sha2::{Digest, Sha256};
        let Ok(root) = root.canonicalize() else {
            return;
        };
        let mut state = self
            .projects
            .iter()
            .position(|p| p.root == root)
            .and_then(|i| self.projects.remove(i))
            .unwrap_or(ProjectState {
                root: root.clone(),
                call_count: 0,
                response_bytes: 0,
                calls: VecDeque::new(),
            });
        let mut files: Vec<FileEmission> = Vec::new();
        let mut current: Option<(String, Vec<String>, [u8; 32])> = None;
        for line in response.lines() {
            let header = line
                .strip_prefix("#### ")
                .and_then(|s| s.split_once(" · ").map(|p| p.0))
                .or_else(|| {
                    line.strip_prefix("## ")
                        .and_then(|s| s.rsplit_once(" at "))
                        .and_then(|(_, s)| s.rsplit_once(':').map(|p| p.0))
                });
            if let Some(path) = header {
                current = None;
                if files.len() >= 24 {
                    continue;
                }
                let Ok(full) = root.join(path).canonicalize() else {
                    continue;
                };
                if !full.starts_with(&root) {
                    continue;
                }
                let Ok(source) = std::fs::read_to_string(full) else {
                    continue;
                };
                let fingerprint = Sha256::digest(source.as_bytes()).into();
                current = Some((
                    path.to_owned(),
                    source.lines().map(str::to_owned).collect(),
                    fingerprint,
                ));
                continue;
            }
            let Some((path, source, fingerprint)) = &current else {
                continue;
            };
            let Some((number, text)) = line.split_once('\t') else {
                continue;
            };
            let Ok(number) = number.parse::<usize>() else {
                continue;
            };
            if number == 0 || source.get(number - 1).map(String::as_str) != Some(text) {
                continue;
            }
            let idx = files
                .iter()
                .position(|f| &f.path == path)
                .unwrap_or_else(|| {
                    files.push(FileEmission {
                        path: path.clone(),
                        ranges: Vec::new(),
                        fingerprint: *fingerprint,
                    });
                    files.len() - 1
                });
            let ranges = &mut files[idx].ranges;
            if let Some(last) = ranges.last_mut() {
                if last.1.checked_add(1) == Some(number) {
                    last.1 = number;
                    continue;
                }
            }
            if ranges.len() < 24 {
                ranges.push((number, number));
            }
        }
        state.call_count = state.call_count.saturating_add(1);
        state.response_bytes = state.response_bytes.saturating_add(response.len());
        state.calls.push_back(CallRecord {
            files,
            response_bytes: response.len(),
        });
        while state.calls.len() > 8 {
            state.calls.pop_front();
        }
        self.projects.push_back(state);
        while self.projects.len() > 4 {
            self.projects.pop_front();
        }
    }
}
