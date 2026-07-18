use anyhow::{Context, Result};
use std::fs::{File, OpenOptions};
use std::io::{BufRead, BufReader, Write};
use std::path::Path;

use movie_radio_types::SegmentEvent;

pub struct EventLogWriter {
    file: File,
}

impl EventLogWriter {
    pub fn create<P: AsRef<Path>>(path: P) -> Result<Self> {
        let path = path.as_ref();
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).with_context(|| {
                format!("failed to create parent directories for {}", path.display())
            })?;
        }
        let file = OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .open(path)
            .with_context(|| format!("failed to create event log at {}", path.display()))?;
        Ok(Self { file })
    }

    pub fn append<P: AsRef<Path>>(path: P) -> Result<Self> {
        let path = path.as_ref();
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).with_context(|| {
                format!("failed to create parent directories for {}", path.display())
            })?;
        }
        let file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(path)
            .with_context(|| format!("failed to append event log at {}", path.display()))?;
        Ok(Self { file })
    }

    pub fn write_event(&mut self, event: &SegmentEvent) -> Result<()> {
        let serialized =
            serde_json::to_string(event).context("failed to serialize SegmentEvent to JSON")?;
        writeln!(self.file, "{}", serialized).context("failed to write event to log file")?;
        Ok(())
    }
}

pub struct EventLogReader {
    reader: BufReader<File>,
}

impl EventLogReader {
    pub fn open<P: AsRef<Path>>(path: P) -> Result<Self> {
        let path = path.as_ref();
        let file = File::open(path)
            .with_context(|| format!("failed to open event log at {}", path.display()))?;
        let reader = BufReader::new(file);
        Ok(Self { reader })
    }

    pub fn read_events(self) -> Result<Vec<SegmentEvent>> {
        let mut events = Vec::new();
        for line in self.reader.lines() {
            let line = line.context("failed to read line from event log")?;
            if line.trim().is_empty() {
                continue;
            }
            let event: SegmentEvent = serde_json::from_str(&line).with_context(|| {
                format!("failed to deserialize SegmentEvent from line: {}", line)
            })?;
            events.push(event);
        }
        Ok(events)
    }

    pub fn stream_events(self) -> impl Iterator<Item = Result<SegmentEvent>> {
        self.reader.lines().map(|line| {
            let l = line.context("failed to read line from event log")?;
            let event: SegmentEvent = serde_json::from_str(&l)
                .with_context(|| format!("failed to deserialize SegmentEvent from line: {}", l))?;
            Ok(event)
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use movie_radio_types::{Segment, SegmentKind};
    use tempfile::tempdir;

    #[test]
    fn test_event_log_roundtrip() -> Result<()> {
        let dir = tempdir()?;
        let log_path = dir.path().join("events.jsonl");

        let event = SegmentEvent::SegmentDetected {
            segment: Segment {
                start_ms: 100,
                end_ms: 200,
                kind: SegmentKind::Speech,
                confidence: 0.95,
                tags: vec!["speech".to_string()],
                prompt: Some("test prompt".to_string()),
            },
        };

        // Write event
        let mut writer = EventLogWriter::create(&log_path)?;
        writer.write_event(&event)?;
        drop(writer);

        // Read event
        let reader = EventLogReader::open(&log_path)?;
        let events = reader.read_events()?;
        assert_eq!(events.len(), 1);
        assert_eq!(events[0], event);

        Ok(())
    }
}
