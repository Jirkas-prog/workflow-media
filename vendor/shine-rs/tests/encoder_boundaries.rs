// Regressions for MP3 quantization and frame completeness.
use shine_rs::{
    quantization,
    types::{L3Loop, ShineGlobalConfig, GRANULE_SIZE},
};

#[test]
fn minimum_mdct_coefficient_can_be_quantized() {
    let mut config = ShineGlobalConfig::default();
    quantization::shine_loop_initialise(&mut config);
    let mut spectrum = [0; GRANULE_SIZE];
    spectrum[0] = i32::MIN;
    let l3loop: &mut L3Loop = &mut config.l3loop;
    l3loop.xr = spectrum.as_mut_ptr();
    for (i, value) in spectrum.iter().enumerate() {
        l3loop.xrabs[i] = quantization::labs(*value);
    }
    l3loop.xrmax = *l3loop.xrabs.iter().max().unwrap();
    let mut quantized = [0; GRANULE_SIZE];
    let max = quantization::quantize_with_l3loop(&mut quantized, -60, l3loop);
    assert!(max > 0);
    assert!(quantized.iter().all(|value| *value >= 0));
}

#[test]
fn ancillary_padding_preserves_frame_boundaries() {
    assert_complete_frames(8_000);
}

#[test]
fn finish_writes_partial_output_word() {
    assert_complete_frames(22_050);
    assert_complete_frames(44_100);
}

#[test]
fn supported_sample_rates_produce_complete_frames() {
    for rate in [
        8_000, 11_025, 12_000, 16_000, 22_050, 24_000, 32_000, 44_100, 48_000,
    ] {
        assert_complete_frames(rate);
    }
}

fn assert_complete_frames(sample_rate: usize) {
    let samples: Vec<i16> = (0..sample_rate)
        .map(|i| {
            ((std::f32::consts::TAU * 440.0 * i as f32 / sample_rate as f32).sin()
                * i16::MAX as f32) as i16
        })
        .collect();
    let config = shine_rs::Mp3EncoderConfig::new()
        .sample_rate(sample_rate as u32)
        .bitrate(64)
        .channels(1)
        .stereo_mode(shine_rs::StereoMode::Mono);
    let encoded = shine_rs::encode_pcm_to_mp3(config, &samples).unwrap();
    let samples_per_frame = if sample_rate >= 32_000 { 1152 } else { 576 };
    let mut offset = 0;
    for _ in 0..samples.len().div_ceil(samples_per_frame) {
        assert!(offset + 4 <= encoded.len(), "missing frame at {offset}");
        assert_eq!(encoded[offset], 0xff, "frame sync at {offset}");
        assert_eq!(encoded[offset + 1] & 0xe0, 0xe0);
        let padding = (encoded[offset + 2] >> 1) & 1;
        offset += samples_per_frame * 64_000 / (8 * sample_rate) + padding as usize;
    }
    assert_eq!(offset, encoded.len(), "incomplete MP3 frame");
}
