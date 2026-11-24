use std::fs::{File, OpenOptions};
use std::io::{self, BufWriter, Seek, SeekFrom, Write};
use std::path::PathBuf;

use serde_json::Value;
use tokio::io::AsyncReadExt;
use zip::write::SimpleFileOptions;
use zip::ZipWriter;

const MAX_ARCHIVE_BYTES: u64 = 2 * 1024 * 1024 * 1024;
const ZIP_BUFFER_BYTES: usize = 64 * 1024;
type ArchiveWriter = ZipWriter<BufWriter<CappedFile>>;

pub struct Archive {
    pub path: PathBuf,
    writer: Option<ArchiveWriter>,
    first_row: bool,
}

impl Archive {
    pub fn new() -> io::Result<Self> {
        let path =
            std::env::temp_dir().join(format!("customer-export-{}.zip", kernel::ids::new_id()));
        let mut options = OpenOptions::new();
        options.write(true).read(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        let file = options.open(&path)?;
        Ok(Self {
            path,
            writer: Some(ZipWriter::new(BufWriter::with_capacity(
                ZIP_BUFFER_BYTES,
                CappedFile(file),
            ))),
            first_row: true,
        })
    }

    fn entry(&mut self, name: &str) -> io::Result<&mut ArchiveWriter> {
        let writer = self
            .writer
            .as_mut()
            .ok_or_else(|| io::Error::other("archive closed"))?;
        writer.start_file(
            name,
            SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored),
        )?;
        Ok(writer)
    }

    pub fn write_json(&mut self, name: &str, rows: &[Value]) -> io::Result<()> {
        serde_json::to_writer(self.entry(name)?, rows).map_err(json_error)
    }

    pub async fn write_attachment(
        &mut self,
        name: &str,
        mut reader: storage::ObjectReader,
    ) -> io::Result<()> {
        let writer = self.entry(name)?;
        let mut buffer = vec![0; 64 * 1024];
        loop {
            let count = reader.read(&mut buffer).await?;
            if count == 0 {
                return Ok(());
            }
            writer.write_all(&buffer[..count])?;
        }
    }

    pub fn finish(&mut self) -> io::Result<()> {
        let mut buffer = self
            .writer
            .take()
            .ok_or_else(|| io::Error::other("archive closed"))?
            .finish()?;
        buffer.flush()?;
        let file = buffer
            .into_inner()
            .map_err(io::IntoInnerError::into_error)?;
        file.0.sync_all()
    }
}

impl db::json_export::JsonExport for Archive {
    fn start(&mut self, name: &str) -> io::Result<()> {
        self.first_row = true;
        self.entry(name)?.write_all(b"[")
    }

    fn row(&mut self, value: &Value) -> io::Result<()> {
        let needs_separator = !self.first_row;
        self.first_row = false;
        let writer = self
            .writer
            .as_mut()
            .ok_or_else(|| io::Error::other("archive closed"))?;
        if needs_separator {
            writer.write_all(b",")?;
        }
        serde_json::to_writer(writer, value).map_err(json_error)
    }

    fn end(&mut self) -> io::Result<()> {
        self.writer
            .as_mut()
            .ok_or_else(|| io::Error::other("archive closed"))?
            .write_all(b"]")
    }
}

impl Drop for Archive {
    fn drop(&mut self) {
        self.writer.take();
        let _ = std::fs::remove_file(&self.path);
    }
}

struct CappedFile(File);

impl Write for CappedFile {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        if self.0.stream_position()?.saturating_add(bytes.len() as u64) > MAX_ARCHIVE_BYTES {
            return Err(io::Error::new(
                io::ErrorKind::FileTooLarge,
                "customer export exceeds 2 GiB",
            ));
        }
        self.0.write(bytes)
    }

    fn flush(&mut self) -> io::Result<()> {
        self.0.flush()
    }
}

impl Seek for CappedFile {
    fn seek(&mut self, from: SeekFrom) -> io::Result<u64> {
        self.0.seek(from)
    }
}

fn json_error(error: serde_json::Error) -> io::Error {
    io::Error::new(
        error.io_error_kind().unwrap_or(io::ErrorKind::InvalidData),
        error,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn archive_buffers_small_writes_until_finish() -> io::Result<()> {
        use db::json_export::JsonExport;

        let mut archive = Archive::new()?;
        archive.start("rows.json")?;
        let length_before_row = std::fs::metadata(&archive.path)?.len();
        archive.row(&serde_json::json!({"id": 42}))?;
        archive.end()?;
        assert_eq!(std::fs::metadata(&archive.path)?.len(), length_before_row);

        archive.finish()?;
        let mut zip = zip::ZipArchive::new(File::open(&archive.path)?)?;
        let rows: Value = serde_json::from_reader(zip.by_name("rows.json")?)?;
        assert_eq!(rows, serde_json::json!([{"id": 42}]));
        Ok(())
    }

    #[test]
    fn archive_finish_refuses_to_cross_the_size_ceiling() -> io::Result<()> {
        let mut archive = Archive::new()?;
        archive.writer.take();
        // Seek without allocating 2 GiB, then make the ZIP footer cross the limit.
        let mut file = CappedFile(OpenOptions::new().write(true).open(&archive.path)?);
        file.seek(SeekFrom::Start(MAX_ARCHIVE_BYTES))?;
        archive.writer = Some(ZipWriter::new(BufWriter::with_capacity(
            ZIP_BUFFER_BYTES,
            file,
        )));
        assert_eq!(
            archive.finish().err().map(|error| error.kind()),
            Some(io::ErrorKind::FileTooLarge)
        );
        Ok(())
    }
}
