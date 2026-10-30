use std::mem;

// Import der Funktionen aus lib.rs
use photocraft_dodge_burn::{pc_abi_version, pc_alloc, pc_filter, pc_manifest};

#[test]
fn test_abi_version() {
    assert_eq!(unsafe { pc_abi_version() }, 1);
}

#[test]
fn test_filter_basic() {
    // Erstelle ein einfaches Testbild: 4x4 RGB (16 Pixel * 3 Kanäle = 48 f32-Werte)
    let width = 4;
    let height = 4;
    let channels = 3;
    let pixel_count = width * height * channels;
    let buf_len = pixel_count * 4; // 4 Bytes pro f32
    
    // Allokiere Speicher für das Bild
    let mut buf = vec![0.0f32; pixel_count];
    
    // Fülle mit Testdaten (Graustufen: alle Kanäle gleich)
    for i in 0..height {
        for j in 0..width {
            let idx = (i * width + j) * channels;
            let value = (i * width + j) as f32 / (width * height) as f32;
            buf[idx] = value;
            buf[idx + 1] = value;
            buf[idx + 2] = value;
        }
    }
    
    // Parameter-JSON
    let params = r#"{"intensity": 0.15, "dodge_strength": 0.5, "burn_strength": 1.0, "blemish_radius": 12, "macro_radius": 55, "preserve_pores": true}"#;
    
    // Rufe pc_filter auf
    let result = unsafe { 
        pc_filter(
            buf.as_mut_ptr() as *mut u8,
            buf_len as i32,
            width as i32,
            height as i32,
            channels as i32,
            0, // format (unwichtig für den Test)
            params.as_ptr() as *const u8,
            params.len() as i32,
        )
    };
    
    assert_eq!(result, 0); // Sollte 0 (Erfolg) zurückgeben
    
    // Prüfe, ob die Ausgabe Werte im Bereich [0, 1] hat
    for &val in &buf {
        assert!(val >= 0.0 && val <= 1.0, "Pixelwert außerhalb des gültigen Bereichs: {}", val);
        assert!(!val.is_nan(), "NaN-Wert in der Ausgabe");
    }
}

#[test]
fn test_filter_preserves_alpha() {
    // RGBA-Bild (4 Kanäle), format-Bit 8 gesetzt: letzter Kanal ist Alpha
    let width = 8;
    let height = 8;
    let channels = 4;
    let pixel_count = width * height * channels;
    let buf_len = pixel_count * 4;

    let mut buf = vec![0.0f32; pixel_count];
    for i in 0..height {
        for j in 0..width {
            let idx = (i * width + j) * channels;
            let value = (i * width + j) as f32 / (width * height) as f32;
            buf[idx] = value;
            buf[idx + 1] = value;
            buf[idx + 2] = value;
            buf[idx + 3] = 1.0; // Alpha: deckend
        }
    }

    let params = r#"{"intensity": 0.8, "dodge_strength": 1.0, "burn_strength": 1.0, "blemish_radius": 12, "macro_radius": 55, "preserve_pores": true}"#;

    let format: i32 = 1 << 8; // Alpha vorhanden
    let result = unsafe {
        pc_filter(
            buf.as_mut_ptr() as *mut u8,
            buf_len as i32,
            width as i32,
            height as i32,
            channels as i32,
            format,
            params.as_ptr() as *const u8,
            params.len() as i32,
        )
    };
    assert_eq!(result, 0);

    for p in 0..width * height {
        let idx = p * channels;
        // Alpha muss unverändert bei 1.0 bleiben (darf nicht die Maske enthalten)
        assert!(
            (buf[idx + 3] - 1.0).abs() < 1e-6,
            "Alpha wurde überschrieben: {}",
            buf[idx + 3]
        );
        // Farbkanäle enthalten die Maske (50% Grau ± 0.3)
        for c in 0..3 {
            let v = buf[idx + c];
            assert!(
                v >= 0.2 - 1e-6 && v <= 0.8 + 1e-6,
                "Maskenwert außerhalb 0.2-0.8: {}",
                v
            );
        }
    }
}

#[test]
fn test_filter_downsampling() {
    // Großes Dokument: adaptives Zellraster, Band mit Kontext-Rändern.
    // Puffer 80x80 = 64x64 Innenbereich + 8 Pixel overlap auf jeder Seite.
    let width = 80;
    let height = 80;
    let overlap = 8usize;
    let channels = 4;
    let pixel_count = width * height * channels;
    let buf_len = pixel_count * 4;

    let mut buf = vec![0.0f32; pixel_count];
    for y in 0..height {
        for x in 0..width {
            let idx = (y * width + x) * channels;
            // Hautfarbener Gradient, damit Korrekturen ungleich null sind
            let v = (x + y) as f32 / 160.0;
            buf[idx] = v;
            buf[idx + 1] = 0.8 * v;
            buf[idx + 2] = 0.7 * v;
            buf[idx + 3] = 1.0; // Alpha: deckend
        }
    }

    let params = format!(
        "{{\"intensity\": 0.8, \"dodge_strength\": 1.0, \"burn_strength\": 1.0, \
\"blemish_radius\": 12, \"macro_radius\": 55, \"preserve_pores\": true, \
\"_image\": {{\"x\": 0, \"y\": 0, \"width\": {}, \"height\": {}, \"overlap\": {}, \
\"canvasWidth\": 12000, \"canvasHeight\": 12000, \"mode\": \"rgb\", \"depth\": 8, \
\"alpha\": true, \"channels\": 4}}}}",
        width, height, overlap
    );

    let format: i32 = 1 << 8; // Alpha vorhanden
    let result = unsafe {
        pc_filter(
            buf.as_mut_ptr() as *mut u8,
            buf_len as i32,
            width as i32,
            height as i32,
            channels as i32,
            format,
            params.as_ptr() as *const u8,
            params.len() as i32,
        )
    };
    assert_eq!(result, 0);

    let mut inner_deviates = false;
    for y in 0..height {
        for x in 0..width {
            let idx = (y * width + x) * channels;
            // Alpha muss unverändert bei 1.0 bleiben
            assert!((buf[idx + 3] - 1.0).abs() < 1e-6, "Alpha überschrieben");

            let inner = x >= overlap && x < width - overlap && y >= overlap && y < height - overlap;
            if inner {
                let v = buf[idx];
                assert!(v >= 0.2 - 1e-6 && v <= 0.8 + 1e-6, "Maskenwert außerhalb 0.2-0.8: {}", v);
                if (v - 0.5).abs() > 1e-4 {
                    inner_deviates = true;
                }
            } else {
                // Kontext-Ränder erhalten konstant 50% Grau
                for c in 0..3 {
                    assert!((buf[idx + c] - 0.5).abs() < 1e-6, "Rand muss 0.5 sein");
                }
            }
        }
    }
    assert!(inner_deviates, "Innenbereich enthält keine Korrektur");
}

#[test]
#[ignore] // Benchmark: `cargo test --test test_plugin --release -- --ignored --nocapture`
fn test_filter_bench_tile() {
    // Realistische Band-Kachel eines 8736x11648-Dokuments (RGBA, overlap 60)
    let width = 632usize;
    let height = 632usize;
    let channels = 4usize;
    let pixel_count = width * height * channels;
    let buf_len = pixel_count * 4;

    let mut buf = vec![0.0f32; pixel_count];
    for y in 0..height {
        for x in 0..width {
            let idx = (y * width + x) * channels;
            let v = ((x * 7 + y * 13) % 256) as f32 / 255.0;
            buf[idx] = v;
            buf[idx + 1] = 0.8 * v;
            buf[idx + 2] = 0.7 * v;
            buf[idx + 3] = 1.0;
        }
    }

    let params = format!(
        "{{\"intensity\": 0.8, \"dodge_strength\": 1.0, \"burn_strength\": 1.0, \
\"blemish_radius\": 12, \"macro_radius\": 55, \"preserve_pores\": true, \
\"_image\": {{\"x\": 0, \"y\": 0, \"width\": {}, \"height\": {}, \"overlap\": 60, \
\"canvasWidth\": 8736, \"canvasHeight\": 11648, \"mode\": \"rgb\", \"depth\": 8, \
\"alpha\": true, \"channels\": 4}}}}",
        width, height
    );

    let format: i32 = 1 << 8;
    let start = std::time::Instant::now();
    let result = unsafe {
        pc_filter(
            buf.as_mut_ptr() as *mut u8,
            buf_len as i32,
            width as i32,
            height as i32,
            channels as i32,
            format,
            params.as_ptr() as *const u8,
            params.len() as i32,
        )
    };
    let elapsed = start.elapsed();
    assert_eq!(result, 0);
    println!(
        "Kachel {}x{} RGBA: {:?} nativ (ca. 388 Kacheln/Dokument)",
        width, height, elapsed
    );
}

#[test]
#[ignore] // Vergleichsmessung: alte Pixel-weise Pipeline (scale == 1)
fn test_filter_bench_tile_fullres() {
    let width = 632usize;
    let height = 632usize;
    let channels = 4usize;
    let pixel_count = width * height * channels;
    let buf_len = pixel_count * 4;

    let mut buf = vec![0.0f32; pixel_count];
    for y in 0..height {
        for x in 0..width {
            let idx = (y * width + x) * channels;
            let v = ((x * 7 + y * 13) % 256) as f32 / 255.0;
            buf[idx] = v;
            buf[idx + 1] = 0.8 * v;
            buf[idx + 2] = 0.7 * v;
            buf[idx + 3] = 1.0;
        }
    }

    // Kein _image -> scale == 1 -> Pipeline pro Pixel wie vor der Optimierung
    let params = r#"{"intensity": 0.8, "dodge_strength": 1.0, "burn_strength": 1.0, "blemish_radius": 12, "macro_radius": 55, "preserve_pores": true}"#;

    let format: i32 = 1 << 8;
    let start = std::time::Instant::now();
    let result = unsafe {
        pc_filter(
            buf.as_mut_ptr() as *mut u8,
            buf_len as i32,
            width as i32,
            height as i32,
            channels as i32,
            format,
            params.as_ptr() as *const u8,
            params.len() as i32,
        )
    };
    let elapsed = start.elapsed();
    assert_eq!(result, 0);
    println!(
        "Kachel {}x{} RGBA (scale=1, pixel-weise): {:?} nativ",
        width, height, elapsed
    );
}
