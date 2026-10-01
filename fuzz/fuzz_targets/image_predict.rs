#![no_main]
libfuzzer_sys::fuzz_target!(|data: &[u8]| {
    let _ = colorvideovdp_fuzz::image_predict(data);
});
