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
