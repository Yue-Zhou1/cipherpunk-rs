use anyhow::{Context, Result, bail};

pub const MAGIC: &[u8; 4] = b"CPKN";
pub const FORMAT_VERSION_V1: u32 = 1;
pub const HEADER_LEN: usize = 0x100;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BlockHeader {
    pub provider: String,
    pub model: String,
    pub dimensions: u32,
    pub signature_count: u32,
    pub vector_offset: u64,
    pub vector_size: u64,
    pub metadata_offset: u64,
    pub metadata_size: u64,
}

impl BlockHeader {
    pub fn parse(bytes: &[u8]) -> Result<Self> {
        if bytes.len() < HEADER_LEN {
            bail!(
                "knowledge.bin header too short: expected {HEADER_LEN} bytes, got {}",
                bytes.len()
            );
        }

        if &bytes[0..4] != MAGIC {
            bail!("invalid memory-block magic (expected CPKN)");
        }

        let version = read_u32(bytes, 0x04)?;
        if version != FORMAT_VERSION_V1 {
            bail!("unsupported memory-block format version {version}");
        }

        Ok(Self {
            provider: parse_null_padded_utf8(&bytes[0x08..0x28], "embedding provider")?,
            model: parse_null_padded_utf8(&bytes[0x28..0x68], "embedding model")?,
            dimensions: read_u32(bytes, 0x68)?,
            signature_count: read_u32(bytes, 0x6C)?,
            vector_offset: read_u64(bytes, 0x70)?,
            vector_size: read_u64(bytes, 0x78)?,
            metadata_offset: read_u64(bytes, 0x80)?,
            metadata_size: read_u64(bytes, 0x88)?,
        })
    }

    pub fn validate_layout(&self, file_len: usize) -> Result<()> {
        let vector_start =
            usize::try_from(self.vector_offset).context("vector offset does not fit in usize")?;
        let vector_end = add_usize(vector_start, self.vector_size as usize)
            .context("vector section overflows file bounds")?;
        let metadata_start = usize::try_from(self.metadata_offset)
            .context("metadata offset does not fit in usize")?;
        let metadata_end = add_usize(metadata_start, self.metadata_size as usize)
            .context("metadata section overflows file bounds")?;

        if vector_end > file_len {
            bail!("vector section out of bounds: end={vector_end}, file_len={file_len}");
        }
        if metadata_end > file_len {
            bail!("metadata section out of bounds: end={metadata_end}, file_len={file_len}");
        }
        if self.vector_offset < HEADER_LEN as u64 {
            bail!(
                "invalid vector offset {}: must be >= header length {}",
                self.vector_offset,
                HEADER_LEN
            );
        }
        if self.metadata_offset < HEADER_LEN as u64 {
            bail!(
                "invalid metadata offset {}: must be >= header length {}",
                self.metadata_offset,
                HEADER_LEN
            );
        }

        let overlaps = vector_start < metadata_end && metadata_start < vector_end;
        if overlaps {
            bail!(
                "vector and metadata sections overlap: vector=[{vector_start},{vector_end}), metadata=[{metadata_start},{metadata_end})"
            );
        }

        Ok(())
    }
}

fn read_u32(bytes: &[u8], start: usize) -> Result<u32> {
    let end = add_usize(start, 4).context("u32 read out of range")?;
    let raw: [u8; 4] = bytes[start..end].try_into().context("convert u32 bytes")?;
    Ok(u32::from_le_bytes(raw))
}

fn read_u64(bytes: &[u8], start: usize) -> Result<u64> {
    let end = add_usize(start, 8).context("u64 read out of range")?;
    let raw: [u8; 8] = bytes[start..end].try_into().context("convert u64 bytes")?;
    Ok(u64::from_le_bytes(raw))
}

fn parse_null_padded_utf8(bytes: &[u8], label: &str) -> Result<String> {
    let end = bytes
        .iter()
        .position(|value| *value == 0)
        .unwrap_or(bytes.len());
    let value = std::str::from_utf8(&bytes[..end]).with_context(|| format!("parse {label}"))?;
    Ok(value.trim().to_string())
}

fn add_usize(left: usize, right: usize) -> Option<usize> {
    left.checked_add(right)
}
