#![no_main]
libfuzzer_sys::fuzz_target!(|data: &[u8]| {
    let _ = colorvideovdp_fuzz::video_predict(data);
});
