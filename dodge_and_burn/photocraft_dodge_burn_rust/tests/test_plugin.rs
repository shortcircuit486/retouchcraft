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

/// S6: Zielmetriken als feste Tests. Referenzwerte kalibriert gegen die echte
/// Retouch4me-Maske (pexels-salim-serdar…, 40 %-Raster):
/// feine Bande (0,5-5 u8) ~47-53%, Burn-Tail >= -9 u8, Werte in [0.2, 0.8],
/// Alpha unangetastet, Band-Ränder exakt 0.5, Korrelation ~0.49.
#[test]
fn test_filter_mask_profile() {
    // Synthetisches Hautbild mit Blemish-Spots, Band-Geometrie wie im Host:
    // Puffer 96x96 = 80x80 Innenbereich + 8 px overlap; Canvas 12000x12000
    // -> scale = min(ceil(sqrt(144M/2M))=9, blemish_radius/3=4) = 4
    let width = 96usize;
    let height = 96usize;
    let overlap = 8usize;
    let channels = 4usize;
    let pixel_count = width * height * channels;
    let buf_len = pixel_count * 4;

    let mut buf = vec![0.0f32; pixel_count];
    for y in 0..height {
        for x in 0..width {
            let idx = (y * width + x) * channels;
            // hautfarbener Verlauf (r > g > b), damit die Hautmaske greift
            let v = 0.30 + 0.45 * ((x + y) as f32 / 192.0);
            buf[idx] = v;
            buf[idx + 1] = 0.8 * v;
            buf[idx + 2] = 0.7 * v;
            buf[idx + 3] = 1.0;
        }
    }
    // Dunkle, gerötete Blemish-Spots (Radius ~4 px)
    for (cx, cy) in [(30usize, 30usize), (60, 40), (40, 65), (70, 70), (25, 60)] {
        for dy in -4i32..=4 {
            for dx in -4i32..=4 {
                if dx * dx + dy * dy > 16 {
                    continue;
                }
                let x = (cx as i32 + dx) as usize;
                let y = (cy as i32 + dy) as usize;
                let idx = (y * width + x) * channels;
                buf[idx] = (buf[idx] * 1.15).min(1.0); // röter
                buf[idx + 1] *= 0.62; // dunkler
                buf[idx + 2] *= 0.62;
            }
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

    let format: i32 = 1 << 8;
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

    let mut n_inner = 0usize;
    let mut n_fine = 0usize;      // 0.5 < |dev| <= 5 u8
    let mut n_dev = 0usize;       // |dev| > 0.5 u8
    let mut min_dev = 0.0f32;
    let mut max_dev = 0.0f32;
    for y in 0..height {
        for x in 0..width {
            let idx = (y * width + x) * channels;
            // Alpha unangetastet
            assert!((buf[idx + 3] - 1.0).abs() < 1e-6, "Alpha überschrieben");

            let inner = x >= overlap && x < width - overlap && y >= overlap && y < height - overlap;
            if inner {
                let v = buf[idx];
                // Wertebereich der Maske
                assert!(v >= 0.2 - 1e-6 && v <= 0.8 + 1e-6, "Maske außerhalb [0.2, 0.8]: {}", v);
                let dev = (v - 0.5) * 255.0;
                n_inner += 1;
                if dev.abs() > 0.5 {
                    n_dev += 1;
                }
                if dev.abs() > 0.5 && dev.abs() <= 5.0 {
                    n_fine += 1;
                }
                min_dev = min_dev.min(dev);
                max_dev = max_dev.max(dev);
            } else {
                // Kontext-Ränder exakt neutral
                for c in 0..3 {
                    assert!((buf[idx + c] - 0.5).abs() < 1e-6, "Rand muss 0.5 sein");
                }
            }
        }
    }

    // Burn-Cap + Floor: Burn-Tail nie unter ca. -11 u8 (Cap -9, Floor -2, Skin <= 1)
    assert!(min_dev >= -11.0, "Burn-Tail unter Cap: {} u8", min_dev);
    // Es gibt Dodge-Aktivität (Spots + Basis)
    assert!(max_dev >= 2.0, "keine Dodge-Spots erkannt, max {} u8", max_dev);
    // Kontinuierliches feines Feld (keine Deadzone): ein nennenswerter Anteil
    // der Innenpixel liegt in der feinen Bande, aber nicht alles
    let fine_frac = n_fine as f32 / n_inner as f32;
    let dev_frac = n_dev as f32 / n_inner as f32;
    assert!(fine_frac >= 0.15 && fine_frac <= 0.95, "feine Bande außerhalb 15-95%: {:.0}%", fine_frac * 100.0);
    assert!(dev_frac >= 0.20, "zu wenig aktive Pixel (Deadzone-Regression?): {:.0}%", dev_frac * 100.0);

    println!(
        "Maskenprofil (synthetisch): fein {:.0}%, aktiv {:.0}%, min {:.1} u8, max {:.1} u8",
        fine_frac * 100.0,
        dev_frac * 100.0,
        min_dev,
        max_dev
    );
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
