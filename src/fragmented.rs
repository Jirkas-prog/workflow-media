//! Fragmented ISO-BMFF packet index. Parsing uses mp4-atom; payload stays on disk.
//! Track descriptors and output timing use containr, shared with ordinary MP4.
use anyhow::{Context, Result, bail};
use containr::{Packet, Track};
use mp4_atom::{Decode, Encode};
use std::{
    collections::HashMap,
    fs::File,
    io::{BufReader, Cursor, Read, Seek, SeekFrom},
    path::Path,
};
pub struct Fragmented {
    file: BufReader<File>,
    pub tracks: Vec<Track>,
    pub packets: Vec<Packet>,
}
struct BoxInfo {
    offset: u64,
    len: u64,
    header: u64,
    kind: [u8; 4],
}
fn scan(file: &mut BufReader<File>) -> Result<Vec<BoxInfo>> {
    let size = file.seek(SeekFrom::End(0))?;
    let mut offset = 0;
    let mut boxes = vec![];
    while offset < size {
        if size - offset < 8 {
            bail!("Truncated MP4 header");
        }
        file.seek(SeekFrom::Start(offset))?;
        let mut b = [0u8; 8];
        file.read_exact(&mut b)?;
        let mut len = u32::from_be_bytes(b[..4].try_into().unwrap()) as u64;
        let mut header = 8;
        if len == 1 {
            let mut large = [0u8; 8];
            file.read_exact(&mut large)?;
            len = u64::from_be_bytes(large);
            header = 16;
        } else if len == 0 {
            len = size - offset;
        }
        if len < header || len > size - offset {
            bail!("Invalid MP4 box length");
        }
        if boxes.len() > 100_000 {
            bail!("Too many MP4 boxes");
        }
        boxes.push(BoxInfo {
            offset,
            len,
            header,
            kind: b[4..8].try_into().unwrap(),
        });
        offset += len;
    }
    Ok(boxes)
}
fn bytes(file: &mut BufReader<File>, b: &BoxInfo) -> Result<Vec<u8>> {
    if b.len > 32 * 1024 * 1024 {
        bail!("MP4 metadata exceeds 32 MB");
    }
    file.seek(SeekFrom::Start(b.offset))?;
    let mut bytes = vec![0; b.len as usize];
    file.read_exact(&mut bytes)?;
    Ok(bytes)
}
impl Fragmented {
    pub fn open(path: &Path) -> Result<Option<Self>> {
        let mut file = BufReader::new(File::open(path)?);
        let boxes = scan(&mut file)?;
        if !boxes.iter().any(|b| b.kind == *b"moof") {
            return Ok(None);
        }
        let moov_box = boxes
            .iter()
            .find(|b| b.kind == *b"moov")
            .context("Missing MP4 initialization")?;
        let moov_bytes = bytes(&mut file, moov_box)?;
        let mut moov = mp4_atom::Moov::decode(&mut moov_bytes.as_slice())?;
        let defaults: HashMap<_, _> = moov
            .mvex
            .take()
            .map(|v| v.trex.into_iter().map(|t| (t.track_id, t)).collect())
            .unwrap_or_default();
        let mut init = vec![];
        for b in boxes.iter().filter(|b| b.kind == *b"ftyp") {
            init.extend(bytes(&mut file, b)?);
        }
        moov.encode(&mut init)?;
        let init = containr::mp4::Demuxer::open(Cursor::new(init))?;
        let mut tracks = init.tracks().to_vec();
        if tracks
            .iter()
            .flat_map(|t| t.sample_entries.iter())
            .any(|e| matches!(&e.fourcc.0, b"encv" | b"enca"))
        {
            bail!("Encrypted MP4 is unsupported / Šifrované MP4 není podporováno");
        }
        let data_ranges: Vec<_> = boxes
            .iter()
            .filter(|b| b.kind == *b"mdat")
            .map(|b| (b.offset + b.header, b.offset + b.len))
            .collect();
        let mut packets = Vec::new();
        let mut decode_end: HashMap<u32, u64> = HashMap::new();
        let mut counts: HashMap<u32, u64> = HashMap::new();
        for box_info in boxes.iter().filter(|b| b.kind == *b"moof") {
            let raw = bytes(&mut file, box_info)?;
            let moof = mp4_atom::Moof::decode(&mut raw.as_slice())?;
            let mut preceding_end = box_info.offset;
            for (traf_index, traf) in moof.traf.into_iter().enumerate() {
                if traf.senc.is_some() {
                    bail!("Encrypted MP4 fragment is unsupported");
                }
                let h = traf.tfhd;
                let id = h.track_id;
                if !tracks.iter().any(|t| t.id == id) {
                    bail!("Unknown MP4 fragment track");
                }
                let default = defaults.get(&id);
                let mut dts = traf
                    .tfdt
                    .map(|t| t.base_media_decode_time)
                    .unwrap_or(*decode_end.get(&id).unwrap_or(&0));
                let base =
                    h.base_data_offset
                        .unwrap_or(if h.default_base_is_moof || traf_index == 0 {
                            box_info.offset
                        } else {
                            preceding_end
                        });
                let mut cursor = base;
                for run in traf.trun {
                    if packets.len().saturating_add(run.samples.len()) > 5_000_000 {
                        bail!("MP4 sample index exceeds the supported size");
                    }
                    if let Some(delta) = run.data_offset {
                        cursor = base
                            .checked_add_signed(delta as i64)
                            .context("Invalid MP4 data offset")?;
                    }
                    for entry in run.samples.iter() {
                        let duration = entry
                            .duration
                            .or(h.default_sample_duration)
                            .or(default.map(|d| d.default_sample_duration))
                            .context("Missing MP4 sample duration")?;
                        let size = entry
                            .size
                            .or(h.default_sample_size)
                            .or(default.map(|d| d.default_sample_size))
                            .context("Missing MP4 sample size")?;
                        let flags = entry
                            .flags
                            .or(h.default_sample_flags)
                            .or(default.map(|d| d.default_sample_flags))
                            .unwrap_or(0);
                        let description = h
                            .sample_description_index
                            .or(default.map(|d| d.default_sample_description_index))
                            .unwrap_or(1);
                        if duration == 0 || size == 0 || size > 128 * 1024 * 1024 {
                            bail!("Invalid MP4 sample size or duration");
                        }
                        let end = cursor
                            .checked_add(size as u64)
                            .context("MP4 sample offset overflow")?;
                        if !data_ranges
                            .iter()
                            .any(|&(start, limit)| cursor >= start && end <= limit)
                        {
                            bail!("MP4 sample extends outside media data");
                        }
                        let decode: i64 = dts.try_into().context("MP4 timestamp overflow")?;
                        let pts = decode
                            .checked_add(entry.cts.unwrap_or(0))
                            .context("MP4 presentation timestamp overflow")?;
                        let index = counts.entry(id).or_insert(0);
                        packets.push(Packet {
                            track_id: id,
                            index: *index,
                            dts: Some(decode),
                            pts,
                            duration: Some(duration as u64),
                            sync: flags & 0x10000 == 0,
                            discardable: false,
                            discard_padding: None,
                            lace: None,
                            offset: cursor,
                            size,
                            description_index: description,
                        });
                        *index += 1;
                        cursor = end;
                        dts = dts
                            .checked_add(duration as u64)
                            .context("MP4 duration overflow")?;
                    }
                }
                preceding_end = cursor;
                decode_end.insert(id, dts);
            }
        }
        for track in &mut tracks {
            track.sample_count = *counts.get(&track.id).unwrap_or(&0);
            track.duration = decode_end.get(&track.id).copied();
        }
        if packets.is_empty() {
            bail!("MP4 contains no samples");
        }
        Ok(Some(Self {
            file,
            tracks,
            packets,
        }))
    }
    pub fn read(&mut self, p: &Packet) -> Result<Vec<u8>> {
        self.file.seek(SeekFrom::Start(p.offset))?;
        let mut bytes = vec![0; p.size as usize];
        self.file.read_exact(&mut bytes)?;
        Ok(bytes)
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn rejects_out_of_bounds_box() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("bad.mp4");
        std::fs::write(&p, b"\xff\xff\xff\xffmoof").unwrap();
        assert!(Fragmented::open(&p).is_err());
    }
}
