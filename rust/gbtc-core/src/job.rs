const MAX_EXTRANONCE1: usize = 16;
const MAX_EXTRANONCE2: usize = 8;
const MAX_JOB_ID: usize = 63;
const MAX_COINBASE: usize = 511;
const MAX_MERKLE: usize = 32;
const MAX_COINBASE_TOTAL: usize = 2048;

#[derive(Debug, Clone)]
pub struct MiningJob {
    pub connection_generation: u64,
    pub job_generation: u64,
    pub target_generation: u64,
    pub job_id: String,
    pub prevhash: [u8; 32],
    pub coinb1: Vec<u8>,
    pub coinb2: Vec<u8>,
    pub merkle: Vec<[u8; 32]>,
    pub version: u32,
    pub nbits: u32,
    pub ntime: u32,
    pub target: [u32; 8],
    pub difficulty: f64,
    pub clean: bool,
}

impl MiningJob {
    pub fn decode_hex(input: &str) -> Result<Vec<u8>, String> {
        decode_hex(input)
    }

    pub fn encode_hex(input: &[u8]) -> String {
        encode_hex(input)
    }

    pub fn header(&self, extranonce1: &[u8], extranonce2: &[u8]) -> Result<[u8; 80], String> {
        if self.job_id.len() > MAX_JOB_ID {
            return Err("job id too long".to_string());
        }
        if extranonce1.len() > MAX_EXTRANONCE1 {
            return Err("extranonce1 too long".to_string());
        }
        if extranonce2.len() > MAX_EXTRANONCE2 {
            return Err("extranonce2 too long".to_string());
        }
        if self.coinb1.len() > MAX_COINBASE || self.coinb2.len() > MAX_COINBASE {
            return Err("coinbase part too long".to_string());
        }
        if self.merkle.len() > MAX_MERKLE {
            return Err("too many merkle branches".to_string());
        }
        let total = self.coinb1.len() + extranonce1.len() + extranonce2.len() + self.coinb2.len();
        if total > MAX_COINBASE_TOTAL {
            return Err("coinbase too long".to_string());
        }

        let mut coinbase = Vec::with_capacity(total);
        coinbase.extend_from_slice(&self.coinb1);
        coinbase.extend_from_slice(extranonce1);
        coinbase.extend_from_slice(extranonce2);
        coinbase.extend_from_slice(&self.coinb2);

        let mut mr = crate::sha256::sha256d(&coinbase);
        for branch in &self.merkle {
            let mut cat = [0u8; 64];
            cat[..32].copy_from_slice(&mr);
            cat[32..].copy_from_slice(branch);
            mr = crate::sha256::sha256d(&cat);
        }

        let mut hdr = [0u8; 80];
        hdr[0] = (self.version & 0xff) as u8;
        hdr[1] = ((self.version >> 8) & 0xff) as u8;
        hdr[2] = ((self.version >> 16) & 0xff) as u8;
        hdr[3] = ((self.version >> 24) & 0xff) as u8;

        for i in 0..8usize {
            hdr[4 + 4 * i] = self.prevhash[4 * i + 3];
            hdr[4 + 4 * i + 1] = self.prevhash[4 * i + 2];
            hdr[4 + 4 * i + 2] = self.prevhash[4 * i + 1];
            hdr[4 + 4 * i + 3] = self.prevhash[4 * i];
        }

        hdr[36..68].copy_from_slice(&mr);

        hdr[68] = (self.ntime & 0xff) as u8;
        hdr[69] = ((self.ntime >> 8) & 0xff) as u8;
        hdr[70] = ((self.ntime >> 16) & 0xff) as u8;
        hdr[71] = ((self.ntime >> 24) & 0xff) as u8;

        hdr[72] = (self.nbits & 0xff) as u8;
        hdr[73] = ((self.nbits >> 8) & 0xff) as u8;
        hdr[74] = ((self.nbits >> 16) & 0xff) as u8;
        hdr[75] = ((self.nbits >> 24) & 0xff) as u8;

        Ok(hdr)
    }

    pub fn work(
        &self,
        extranonce1: &[u8],
        extranonce2: &[u8],
        base: u32,
        count: u32,
    ) -> Result<crate::backend::WorkBatch, String> {
        if !self.difficulty.is_finite() || self.difficulty < 1.0 || self.difficulty > 1e30 {
            return Err("difficulty must be finite and from 1 to 1e30".to_string());
        }
        let header = self.header(extranonce1, extranonce2)?;
        if count == 0 {
            return Err("nonce count must be greater than zero".to_string());
        }
        let end = base as u64 + count as u64;
        if end > (1u64 << 32) {
            return Err("nonce range exceeds 2^32".to_string());
        }
        let (midstate, tail3) = crate::work::build_header_midstate(&header)?;
        Ok(crate::backend::WorkBatch {
            midstate,
            tail3,
            target: self.target,
            nonce_base: base,
            nonce_count: count,
        })
    }
}

pub fn decode_hex(input: &str) -> Result<Vec<u8>, String> {
    if !input.len().is_multiple_of(2) {
        return Err("hex length must be even".to_string());
    }
    let mut out = Vec::with_capacity(input.len() / 2);
    for i in (0..input.len()).step_by(2) {
        let hi = hex_value(input.as_bytes()[i])?;
        let lo = hex_value(input.as_bytes()[i + 1])?;
        out.push((hi << 4) | lo);
    }
    Ok(out)
}

pub fn encode_hex(input: &[u8]) -> String {
    const L: &[u8] = b"0123456789abcdef";
    let mut s = String::with_capacity(input.len() * 2);
    for &x in input {
        s.push(L[(x >> 4) as usize] as char);
        s.push(L[(x & 0x0f) as usize] as char);
    }
    s
}

fn hex_value(c: u8) -> Result<u8, String> {
    match c {
        b'0'..=b'9' => Ok(c - b'0'),
        b'a'..=b'f' => Ok(c - b'a' + 10),
        b'A'..=b'F' => Ok(c - b'A' + 10),
        _ => Err("invalid hex character".to_string()),
    }
}
