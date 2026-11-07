//! Pure Rust container stream-copy and audio encoding.
use anyhow::{Context, Result, bail};
use containr::{Packet, Track, TrackKind};
use std::{
    fs::{self, File},
    io::{Read, Write},
    path::{Path, PathBuf},
    sync::atomic::{AtomicBool, Ordering},
};
enum Input {
    Mp4(containr::mp4::Demuxer<std::io::BufReader<File>>),
    Mkv(containr::mkv::Demuxer<std::io::BufReader<File>>),
    Fragmented(crate::fragmented::Fragmented),
}
impl Input {
    fn open(path: &Path) -> Result<Self> {
        let mut header = [0u8; 4];
        File::open(path)?.read_exact(&mut header)?;
        if header == [0x1a, 0x45, 0xdf, 0xa3] {
            Ok(Self::Mkv(containr::mkv::Demuxer::open_path(path)?))
        } else if let Some(d) = crate::fragmented::Fragmented::open(path)? {
            Ok(Self::Fragmented(d))
        } else {
            Ok(Self::Mp4(containr::mp4::Demuxer::open_path(path)?))
        }
    }
    fn tracks(&self) -> &[Track] {
        match self {
            Self::Mp4(d) => d.tracks(),
            Self::Mkv(d) => d.tracks(),
            Self::Fragmented(d) => &d.tracks,
        }
    }
    fn packets(&self) -> Result<Vec<Packet>> {
        Ok(match self {
            Self::Mp4(d) => d.packets().collect::<std::result::Result<Vec<_>, _>>()?,
            Self::Mkv(d) => d.packets().collect::<std::result::Result<Vec<_>, _>>()?,
            Self::Fragmented(d) => d.packets.clone(),
        })
    }
    fn read(&mut self, packet: &Packet) -> Result<Vec<u8>> {
        Ok(match self {
            Self::Mp4(d) => d.read_packet(packet)?,
            Self::Mkv(d) => d.read_packet(packet)?,
            Self::Fragmented(d) => d.read(packet)?,
        })
    }
}
struct Stream {
    input: usize,
    track: Track,
    packets: Vec<Packet>,
}
pub fn merge(files: &[PathBuf], output: &Path, format: &str, cancel: &AtomicBool) -> Result<()> {
    let mut inputs: Vec<_> = files
        .iter()
        .map(|p| Input::open(p))
        .collect::<Result<_>>()?;
    let mut streams = vec![];
    for (i, input) in inputs.iter().enumerate() {
        let packets = input.packets()?;
        for track in input
            .tracks()
            .iter()
            .filter(|t| matches!(t.kind, TrackKind::Audio | TrackKind::Video) && !t.chapter_track)
        {
            let packets = packets
                .iter()
                .filter(|p| p.track_id == track.id)
                .copied()
                .collect();
            streams.push(Stream {
                input: i,
                track: track.clone(),
                packets,
            });
        }
    }
    if streams.is_empty() {
        bail!("No media tracks / Chybí mediální stopy");
    }
    // Interleave by integer decode timestamps, keeping every track's original timing.
    let mut order: Vec<(usize, usize)> = streams
        .iter()
        .enumerate()
        .flat_map(|(i, s)| (0..s.packets.len()).map(move |p| (i, p)))
        .collect();
    order.sort_by(|&(a, pa), &(b, pb)| {
        let ta = &streams[a].track.timebase;
        let tb = &streams[b].track.timebase;
        let pa = &streams[a].packets[pa];
        let pb = &streams[b].packets[pb];
        let left = pa.dts.unwrap_or(pa.pts) as i128 * ta.num as i128 * tb.den as i128;
        let right = pb.dts.unwrap_or(pb.pts) as i128 * tb.num as i128 * ta.den as i128;
        left.cmp(&right)
    });
    let file = File::options()
        .create(true)
        .truncate(true)
        .read(true)
        .write(true)
        .open(output)?;
    if format == "MKV" {
        let plans: Vec<_> = streams
            .iter()
            .map(|s| {
                if s.track.matroska.is_some() {
                    containr::mkv::StreamCopy::plan(&s.track, &s.packets)
                } else {
                    containr::mkv::StreamCopy::plan_rounded(&s.track, &s.packets, 1_000_000)
                }
            })
            .collect::<std::result::Result<_, _>>()?;
        let options = containr::mkv::MkvOptions {
            timestamp_scale: containr::mkv::StreamCopy::timestamp_scale(&plans),
            ..Default::default()
        };
        let mut out = containr::mkv::Muxer::new(file, options)?;
        let ids: Vec<_> = plans
            .iter()
            .map(|p| out.add_track(p.spec.clone()))
            .collect::<std::result::Result<_, _>>()?;
        for (i, p) in order {
            if cancel.load(Ordering::Relaxed) {
                bail!("@cancelled");
            }
            let packet = &streams[i].packets[p];
            let bytes = inputs[streams[i].input].read(packet)?;
            if let Some(block) = plans[i].packet(packet, &bytes)? {
                out.write_packet(ids[i], &block)?;
            }
        }
        out.finish()?;
    } else {
        let mut scale = 1000u64;
        for s in &streams {
            for value in [s.track.timebase.den, s.track.edit_list.movie_timescale] {
                if value > 0 {
                    let (mut a, mut b) = (scale, value as u64);
                    while b != 0 {
                        let rem = a % b;
                        a = b;
                        b = rem;
                    }
                    scale = (scale / a)
                        .checked_mul(value as u64)
                        .context("Movie timescale overflow")?;
                }
            }
        }
        let options = containr::mp4::MuxOptions {
            faststart: true,
            movie_timescale: scale.try_into().context("Incompatible stream timescales")?,
            ..Default::default()
        };
        let plans: Vec<_> = streams
            .iter()
            .map(|s| containr::mp4::StreamCopy::plan(&s.track, &s.packets, options.movie_timescale))
            .collect::<std::result::Result<_, _>>()?;
        let mut out = containr::mp4::Muxer::new(file, options)?;
        let ids: Vec<_> = plans
            .iter()
            .map(|p| out.add_track(p.spec.clone()))
            .collect::<std::result::Result<_, _>>()?;
        for (i, p) in order {
            if cancel.load(Ordering::Relaxed) {
                bail!("@cancelled");
            }
            let packet = &streams[i].packets[p];
            let bytes = inputs[streams[i].input].read(packet)?;
            out.write_packet(ids[i], &plans[i].packet(packet, &bytes)?)?;
        }
        out.finish()?;
    }
    if fs::metadata(output)?.len() == 0 {
        bail!("Empty output file / Prázdný výsledný soubor");
    }
    Ok(())
}
pub fn mp3(input: &Path, output: &Path, cancel: &AtomicBool) -> Result<()> {
    use symphonia::core::{
        audio::SampleBuffer, codecs::DecoderOptions, errors::Error, formats::FormatOptions,
        io::MediaSourceStream, meta::MetadataOptions, probe::Hint,
    };
    let stream = MediaSourceStream::new(Box::new(File::open(input)?), Default::default());
    let mut hint = Hint::new();
    if let Some(ext) = input.extension().and_then(|e| e.to_str()) {
        hint.with_extension(ext);
    }
    let probed = symphonia::default::get_probe().format(
        &hint,
        stream,
        &FormatOptions::default(),
        &MetadataOptions::default(),
    )?;
    let mut demux = probed.format;
    let track = demux
        .tracks()
        .iter()
        .find(|t| t.codec_params.sample_rate.is_some())
        .context("No supported audio track / Chybí podporovaná zvuková stopa")?;
    let id = track.id;
    let params = track.codec_params.clone();
    let mut encoder: Option<shine_rs::Mp3Encoder> = None;
    let mut decoder = symphonia::default::get_codecs().make(&params, &DecoderOptions::default())?;
    let mut out = File::create(output)?;
    let mut frames = 0usize;
    loop {
        if cancel.load(Ordering::Relaxed) {
            bail!("@cancelled");
        }
        let packet = match demux.next_packet() {
            Ok(p) => p,
            Err(Error::IoError(e)) if e.kind() == std::io::ErrorKind::UnexpectedEof => break,
            Err(e) => return Err(e.into()),
        };
        if packet.track_id() != id {
            continue;
        }
        let decoded = decoder.decode(&packet)?;
        if encoder.is_none() {
            let rate = decoded.spec().rate;
            let channels = decoded.spec().channels.count();
            if channels > 2 {
                bail!(
                    "MP3 currently supports mono and stereo inputs / MP3 podporuje mono a stereo vstup"
                );
            }
            let bitrate = if rate >= 32000 {
                320
            } else if rate >= 16000 {
                160
            } else {
                64
            };
            let config = shine_rs::Mp3EncoderConfig::new()
                .sample_rate(rate)
                .channels(channels as u8)
                .bitrate(bitrate)
                .stereo_mode(if channels == 1 {
                    shine_rs::StereoMode::Mono
                } else {
                    shine_rs::StereoMode::Stereo
                });
            encoder = Some(shine_rs::Mp3Encoder::new(config)?);
        }
        let mut buffer = SampleBuffer::<i16>::new(decoded.capacity() as u64, *decoded.spec());
        buffer.copy_interleaved_ref(decoded);
        for block in encoder
            .as_mut()
            .unwrap()
            .encode_interleaved(buffer.samples())?
        {
            out.write_all(&block)?;
        }
        frames += buffer.samples().len();
    }
    if frames == 0 {
        bail!("No audio decoded / Nebyl dekódován žádný zvuk");
    }
    out.write_all(&encoder.as_mut().unwrap().finish()?)?;
    out.sync_all()?;
    Ok(())
}
