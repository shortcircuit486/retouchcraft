// Photocraft Dodge & Burn Plugin (WASM)
// =====================================
// ABI v1-kompatibles Plugin für Photocraft
// Erzeugt 50% Neutralgrau-Masken für Dodge & Burn Korrekturen
//
// Datum: 29. Oktober 2026
// Version: 1.1.0 (korrigiert: Bounds-Checking, Fehlerbehandlung)

// Standard-Bibliothek verwenden
use std::slice;

// ============================================================================
// KONSTANTEN
// ============================================================================

/// ABI Version (muss 1 sein)
const ABI_VERSION: i32 = 1;

/// Maximale Speichergröße: 512 MiB (Photocraft-Limit)
const MAX_MEMORY: usize = 512 * 1024 * 1024; // 536870912 Bytes

/// Manifest-JSON (statisch eingebettet)
const MANIFEST: &str = r#"{
  "id": "photocraft.dodge_burn",
  "name": "Dodge & Burn Mask",
  "version": "1.1.0",
  "kind": "filter",
  "description": "Generiert eine 50% Neutralgrau-Maske für Dodge & Burn Korrekturen. Fügen Sie die Maske als Ebene mit Soft-Light-Modus ein.",
  "params": {
    "intensity": {
      "type": "number",
      "min": 0.0,
      "max": 0.3,
      "default": 0.15,
      "unit": "none",
      "description": "Gesamtstärke der Korrektur (empfohlen: 0.05-0.2)"
    },
    "dodge_strength": {
      "type": "number",
      "min": 0.0,
      "max": 2.0,
      "default": 0.5,
      "unit": "none",
      "description": "Stärke der Aufhellung (Dodge) - empfohlen: 0.3-0.8"
    },
    "burn_strength": {
      "type": "number",
      "min": 0.0,
      "max": 2.0,
      "default": 1.0,
      "unit": "none",
      "description": "Stärke der Abdunklung (Burn) - empfohlen: 0.8-1.2"
    },
    "blemish_radius": {
      "type": "int",
      "min": 1,
      "max": 200,
      "default": 12,
      "unit": "px",
      "description": "Radius für lokale Unebenheiten (Pickel, Rötungen)"
    },
    "macro_radius": {
      "type": "int",
      "min": 1,
      "max": 500,
      "default": 55,
      "unit": "px",
      "description": "Radius für globale Gesichtsform"
    },
    "preserve_pores": {
      "type": "bool",
      "default": true,
      "description": "Feine Porenstruktur erhalten (empfohlen)"
    }
  },
  "overlap": 55,
  "area": "canvas"
}"#;

// ============================================================================
// SPEICHERMANAGEMENT
// ============================================================================

/// Maximale Speichergröße: 512 MiB (Photocraft-Limit)
/// Wir verwenden den Rust-Heap, der im linearen WASM-Speicher liegt.
/// pc_alloc allokiert Speicher auf dem Heap und gibt die Zeiger zurück.
/// Der Speicher wird nie freigegeben (wie von der ABI verlangt).

/// Trackt den insgesamt allokierten Speicher, um das 512 MiB Limit einzuhalten
static mut TOTAL_ALLOCATED: usize = 0;

// ============================================================================
// HILFSFUNKTIONEN
// ============================================================================

/// Parst einen f32-Parameter aus dem JSON-String
fn parse_f32_param(params: &str, key: &str, default: f32) -> f32 {
    // Einfache Suche nach dem Parameter (ohne vollständiges JSON-Parsing)
    let search_str = format!("\"{}\":", key);
    if let Some(start) = params.find(&search_str) {
        let remaining = &params[start + search_str.len()..];
        if let Some(end) = remaining.find(|c: char| !c.is_ascii_digit() && c != '.' && c != '-' && c != '+') {
            let value_str = remaining[..end].trim();
            if let Ok(value) = value_str.parse::<f32>() {
                return value;
            }
        }
    }
    default
}

/// Parst einen i32-Parameter aus dem JSON-String
fn parse_i32_param(params: &str, key: &str, default: i32) -> i32 {
    let search_str = format!("\"{}\":", key);
    if let Some(start) = params.find(&search_str) {
        let remaining = &params[start + search_str.len()..];
        if let Some(end) = remaining.find(|c: char| !c.is_ascii_digit() && c != '-') {
            let value_str = remaining[..end].trim();
            if let Ok(value) = value_str.parse::<i32>() {
                return value;
            }
        }
    }
    default
}

/// Parst einen bool-Parameter aus dem JSON-String
fn parse_bool_param(params: &str, key: &str, default: bool) -> bool {
    let search_str = format!("\"{}\":", key);
    if let Some(start) = params.find(&search_str) {
        let remaining = &params[start + search_str.len()..];
        if let Some(end) = remaining.find(|c: char| !c.is_ascii_alphabetic()) {
            let value_str = remaining[..end].trim();
            if value_str == "true" {
                return true;
            } else if value_str == "false" {
                return false;
            }
        }
    }
    default
}

// ============================================================================
// KERN-ALGORITHMEN
// ============================================================================

/// Berechnet den Mittelwert eines Bereichs (Box-Filter)
/// Verwendet kumulative Summen für O(1) Performance
fn box_filter(img: &[f32], width: usize, height: usize, r: usize) -> Vec<f32> {
    let r = r.max(1);
    let mut output = vec![0.0f32; width * height];
    
    // Erzeuge kumulative Summen in x-Richtung
    let mut integral = vec![0.0f32; (width + 1) * (height + 1)];
    
    // Fülle Integralbild
    for y in 0..height {
        let mut row_sum = 0.0f32;
        for x in 0..width {
            row_sum += img[y * width + x];
            integral[(y + 1) * (width + 1) + x + 1] = integral[y * (width + 1) + x + 1] + row_sum;
        }
    }
    
    // Berechne Box-Filter mit Integralbild
    let r_i32 = r as i32;
    let area = (2 * r_i32 + 1) as f32 * (2 * r_i32 + 1) as f32;
    
    for y in 0..height {
        for x in 0..width {
            let x_i32 = x as i32;
            let y_i32 = y as i32;
            let x1 = (x_i32 - r_i32).max(0) as usize;
            let x2 = ((x_i32 + r_i32 + 1) as usize).min(width);
            let y1 = (y_i32 - r_i32).max(0) as usize;
            let y2 = ((y_i32 + r_i32 + 1) as usize).min(height);
            
            let sum = integral[y2 * (width + 1) + x2] 
                    + integral[y1 * (width + 1) + x1] 
                    - integral[y2 * (width + 1) + x1] 
                    - integral[y1 * (width + 1) + x2];
            
            output[y * width + x] = sum / area;
        }
    }
    
    output
}

/// Guided Filter (kantenerhaltend) nach He et al.
/// I: Guidance-Bild, p: Zu filterndes Bild, r: Radius, eps: Regularisierung
fn guided_filter(i: &[f32], p: &[f32], width: usize, height: usize, r: usize, eps: f32) -> Vec<f32> {
    // Schritt 1: Berechne Mittelwerte
    let mean_i = box_filter(i, width, height, r);
    let mean_p = box_filter(p, width, height, r);
    
    // Schritt 2: Berechne I*I und I*p
    let mut i_squared = vec![0.0f32; width * height];
    let mut i_times_p = vec![0.0f32; width * height];
    for idx in 0..width * height {
        i_squared[idx] = i[idx] * i[idx];
        i_times_p[idx] = i[idx] * p[idx];
    }
    
    let corr_i = box_filter(&i_squared, width, height, r);
    let corr_ip = box_filter(&i_times_p, width, height, r);
    
    // Schritt 3: Berechne Varianz und Kovarianz
    let mut var_i = vec![0.0f32; width * height];
    let mut cov_ip = vec![0.0f32; width * height];
    for idx in 0..width * height {
        var_i[idx] = (corr_i[idx] - mean_i[idx] * mean_i[idx]).max(0.0);
        cov_ip[idx] = corr_ip[idx] - mean_i[idx] * mean_p[idx];
    }
    
    // Schritt 4: Berechne Filterkoeffizienten a und b
    let mut a = vec![0.0f32; width * height];
    let mut b = vec![0.0f32; width * height];
    for idx in 0..width * height {
        let denominator = var_i[idx] + eps;
        // Vermeide NaN durch sichere Division
        if denominator > 1e-10 {
            a[idx] = cov_ip[idx] / denominator;
        } else {
            a[idx] = 0.0;
        }
        b[idx] = mean_p[idx] - a[idx] * mean_i[idx];
        // Ersetze NaN-Werte durch 0
        if a[idx].is_nan() { a[idx] = 0.0; }
        if b[idx].is_nan() { b[idx] = 0.0; }
    }
    
    // Schritt 5: Filtere a und b
    let mean_a = box_filter(&a, width, height, r);
    let mean_b = box_filter(&b, width, height, r);
    
    // Schritt 6: Berechne das gefilterte Bild
    let mut output = vec![0.0f32; width * height];
    for idx in 0..width * height {
        output[idx] = mean_a[idx] * i[idx] + mean_b[idx];
    }
    
    output
}

/// Berechnet die Luminanz aus RGB
#[inline]
fn rgb_to_luminance(r: f32, g: f32, b: f32) -> f32 {
    0.299 * r + 0.587 * g + 0.114 * b
}

/// Berechnet die Haut-Wahrscheinlichkeit (YCbCr + RGB-Regeln)
#[inline]
fn compute_skin_probability(r: f32, g: f32, b: f32) -> f32 {
    // Ersetze NaN-Werte durch 0
    let r = if r.is_nan() { 0.0 } else { r.clamp(0.0, 1.0) };
    let g = if g.is_nan() { 0.0 } else { g.clamp(0.0, 1.0) };
    let b = if b.is_nan() { 0.0 } else { b.clamp(0.0, 1.0) };
    
    // YCbCr Konvertierung (ITU-R BT.601)
    let y = 0.299 * r + 0.587 * g + 0.114 * b;
    let cb = -0.168736 * r - 0.331264 * g + 0.5 * b + 0.5;
    let cr = 0.5 * r - 0.418688 * g - 0.081312 * b + 0.5;
    
    // Skalierung auf 0-1 (YCbCr in Photocraft ist normalisiert)
    let y_scaled = y * 255.0;
    let cb_scaled = cb * 255.0;
    let cr_scaled = cr * 255.0;
    
    // Haut-Cluster im Cb/Cr-Raum
    // Typische menschliche Haut: Cr ~ 148-155, Cb ~ 102-110
    let dist_sq = ((cr_scaled - 150.0) / 22.0).powi(2) + ((cb_scaled - 106.0) / 18.0).powi(2);
    let prob = (-0.5 * dist_sq).exp();
    
    // Heuristik: R > G > B für Haut
    let rgb_rule = if r > g && g > b && (r - g) > 0.03 && y_scaled > 25.0 && y_scaled < 240.0 {
        1.0
    } else {
        0.0
    };
    
    // Lippen-Unterdrückung (hohes Cr/Cb-Verhältnis)
    let lip_suppression = ((cr_scaled - cb_scaled - 48.0) / 22.0).max(0.0).min(1.0);
    let lip_suppression = 1.0 - lip_suppression;
    
    // Augen-/Pupillen-Ausschluss (dunkle oder ungesättigte Bereiche)
    let max_c = r.max(g).max(b);
    let min_c = r.min(g).min(b);
    let sat = if max_c > 1e-4 { (max_c - min_c) / max_c.max(1e-4) } else { 0.0 };
    let eye_suppression = ((sat - 0.05) / 0.1).max(0.0).min(1.0);
    
    // Kombiniere alle Faktoren
    let result = prob * rgb_rule * lip_suppression * eye_suppression;
    if result.is_nan() {
        0.0
    } else {
        result.min(1.0).max(0.0)
    }
}

// ============================================================================
// ABI v1 EXPORTS
// ============================================================================

/// ABI Version (muss 1 zurückgeben)
#[no_mangle]
pub extern "C" fn pc_abi_version() -> i32 {
    ABI_VERSION
}

/// Manifest (gibt Zeiger und Länge zurück)
#[no_mangle]
pub extern "C" fn pc_manifest() -> i64 {
    unsafe {
        let manifest_bytes = MANIFEST.as_bytes();
        let len = manifest_bytes.len() as i32;
        
        // Allokiere Speicher für das Manifest
        let ptr = pc_alloc(len as u32);
        
        if ptr.is_null() {
            return 0; // Fehler: Kein Speicher
        }
        
        // Kopiere Manifest in den allokierten Speicher
        for i in 0..len as usize {
            *ptr.add(i) = manifest_bytes[i];
        }
        
        // Rückgabe: (len << 32) | ptr
        ((len as i64) << 32) | (ptr as i64)
    }
}

/// Speicherallokator (gibt Zeiger auf allokierten Speicher zurück)
/// Gibt 0 zurück bei Fehlern
/// 
/// Allokiert Speicher auf dem Rust-Heap (WASM-linearer Speicher).
/// Der Speicher wird nie freigegeben (wie von der ABI verlangt).
#[no_mangle]
pub extern "C" fn pc_alloc(size: u32) -> *mut u8 {
    let size_usize = size as usize;
    
    // Prüfe auf gültige Größe
    if size_usize == 0 {
        return 0 as *mut u8;
    }
    
    unsafe {
        // Prüfe Bounds: insgesamt allokierter Speicher darf MAX_MEMORY nicht überschreiten
        if TOTAL_ALLOCATED + size_usize > MAX_MEMORY {
            return 0 as *mut u8; // Out of memory
        }
        
        // Allokiere auf dem Heap
        let mut buf = Vec::with_capacity(size_usize);
        let ptr: *mut u8 = buf.as_mut_ptr();
        
        // Fülle mit Nullen (nicht streng notwendig, aber sicher)
        for i in 0..size_usize {
            *ptr.add(i) = 0;
        }
        
        // Verhindere, dass der Vec beim Verlassen der Funktion dropped wird
        std::mem::forget(buf);
        
        TOTAL_ALLOCATED += size_usize;
        ptr
    }
}

/// Hauptfilter-Funktion (ABI v1)
/// Verarbeitet Pixel in-place und gibt 0 bei Erfolg zurück
#[no_mangle]
pub extern "C" fn pc_filter(
    buf: *mut u8,
    buf_len: i32,
    width: i32,
    height: i32,
    channels: i32,
    _format: i32,
    params: *const u8,
    params_len: i32,
) -> i32 {
    // === 1. NULL-Prüfung ===
    if buf.is_null() {
        return 1; // Fehler: buf ist NULL
    }
    if params.is_null() && params_len > 0 {
        return 2; // Fehler: params ist NULL aber params_len > 0
    }
    
    // === 2. Parameter-Validierung ===
    if width <= 0 || height <= 0 || channels <= 0 {
        return 3; // Fehler: Ungültige Dimensionen
    }
    
    let width_usize = width as usize;
    let height_usize = height as usize;
    let channels_usize = channels as usize;
    
    // === 2b. Überprüfe auf Overflow bei der Berechnung von pixel_count ===
    // Maximal zulässige Pixel: MAX_MEMORY / 4 (da jedes Pixel 4 Bytes ist)
    const MAX_PIXELS: usize = MAX_MEMORY / 4;
    
    // Prüfe, ob width * height * channels MAX_PIXELS überschreiten würde
    if width_usize > MAX_PIXELS / height_usize.max(1) / channels_usize.max(1) {
        return 5; // Fehler: Bild zu groß
    }
    
    // Berechne erwartete Pixel-Anzahl
    let expected_pixel_count = width_usize * height_usize * channels_usize;
    let expected_byte_count = expected_pixel_count * 4; // f32 = 4 Bytes
    
    // Prüfe buf_len
    if (buf_len as usize) < expected_byte_count {
        return 4; // Fehler: buf zu klein
    }
    
    // === 3. Parameter parsen ===
    let params_str = if params_len > 0 && !params.is_null() {
        let params_slice = unsafe { slice::from_raw_parts(params, params_len as usize) };
        std::str::from_utf8(params_slice).unwrap_or("")
    } else {
        ""
    };
    
    // Standardwerte
    let intensity = parse_f32_param(params_str, "intensity", 0.15);
    let dodge_strength = parse_f32_param(params_str, "dodge_strength", 0.5);
    let burn_strength = parse_f32_param(params_str, "burn_strength", 1.0);
    let blemish_radius = parse_i32_param(params_str, "blemish_radius", 12);
    let macro_radius = parse_i32_param(params_str, "macro_radius", 55);
    let _preserve_pores = parse_bool_param(params_str, "preserve_pores", true);
    
    // === 4. Pixel-Buffer als f32 interpretieren ===
    let pixel_count = expected_pixel_count;
    let pixels = unsafe { slice::from_raw_parts_mut(buf as *mut f32, pixel_count) };
    
    // === 4b. NaN-Werte in Eingabedaten ersetzen (Photocraft könnte NaN für undefinierte Bereiche verwenden) ===
    for p in pixels.iter_mut() {
        if p.is_nan() {
            *p = 0.0;
        }
    }
    
    // === 5. Luminanz berechnen ===
    let mut lum = vec![0.0f32; width_usize * height_usize];
    for i in 0..height_usize {
        for j in 0..width_usize {
            let idx = (i * width_usize + j) * channels_usize;
            if channels_usize >= 3 {
                let r = pixels[idx];
                let g = pixels[idx + 1];
                let b = pixels[idx + 2];
                lum[i * width_usize + j] = rgb_to_luminance(r, g, b);
            } else {
                // Graustufenbild
                lum[i * width_usize + j] = pixels[idx];
            }
        }
    }
    
    // === 6. Hautmaske berechnen ===
    let mut skin_mask = vec![0.0f32; width_usize * height_usize];
    for i in 0..height_usize {
        for j in 0..width_usize {
            let idx = (i * width_usize + j) * channels_usize;
            if channels_usize >= 3 {
                let r = pixels[idx];
                let g = pixels[idx + 1];
                let b = pixels[idx + 2];
                skin_mask[i * width_usize + j] = compute_skin_probability(r, g, b);
            } else {
                skin_mask[i * width_usize + j] = 1.0; // Bei Graustufen: Alles als Haut
            }
        }
    }
    
    // === 7. Multi-Skalen-Frequenztrennung ===
    let blemish_radius_usize = blemish_radius as usize;
    let macro_radius_usize = macro_radius as usize;
    
    // Makro-Ebene: Grobe Gesichtsform
    let macro_lum = guided_filter(&lum, &lum, width_usize, height_usize, macro_radius_usize, 0.03);
    
    // Mittlere Ebene: Lokale Unebenheiten
    let mid_lum = guided_filter(&lum, &lum, width_usize, height_usize, blemish_radius_usize, 0.006);
    
    // === 8. Dodge & Burn Korrektur berechnen ===
    let mut correction = vec![0.0f32; width_usize * height_usize];
    for i in 0..height_usize {
        for j in 0..width_usize {
            let idx = i * width_usize + j;
            let delta = macro_lum[idx] - mid_lum[idx];
            
            // Asymmetrische Gewichtung
            let dodge = delta.max(0.0) * dodge_strength;
            let burn = delta.min(0.0) * burn_strength;
            
            // Kombiniere Dodge und Burn
            correction[idx] = (dodge + burn) * intensity;
        }
    }
    
    // === 9. Maskierung mit Haut-Wahrscheinlichkeit ===
    for i in 0..height_usize {
        for j in 0..width_usize {
            let idx = i * width_usize + j;
            correction[idx] *= skin_mask[idx];
        }
    }
    
    // === 10. Zentrierung auf 50% Neutralgrau (0.5) ===
    // OPTIMIERUNG: Skalierung auf 10% der ursprünglichen Stärke
    // und Begrenzung auf ±5% um 0.5 (128 ± 12.8 → 115-141)
    for i in 0..height_usize {
        for j in 0..width_usize {
            let idx = i * width_usize + j;
            
            // Skalierungsfaktor 0.1 für professionelle Ergebnisse
            let scaled_correction = correction[idx] * 0.1;
            
            // Begrenze auf ±0.05 (5% um 0.5)
            let clamped_correction = scaled_correction.max(-0.05).min(0.05);
            
            // 50% Neutralgrau + Korrektur
            let mask_value = 0.5 + clamped_correction;
            
            // Schreibe den Maskenwert in alle Kanäle
            let pixel_idx = (i * width_usize + j) * channels_usize;
            for c in 0..channels_usize {
                pixels[pixel_idx + c] = mask_value;
            }
        }
    }
    
    // === 11. Erfolg zurückgeben ===
    0
}

// ============================================================================
// TESTFUNKTIONEN (für lokale Tests)
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_box_filter() {
        // Einfaches Testbild: 3x3 mit Werten 1-9
        let img = vec![
            1.0, 2.0, 3.0,
            4.0, 5.0, 6.0,
            7.0, 8.0, 9.0
        ];
        
        let result = box_filter(&img, 3, 3, 1);
        
        // Mittelwert aller 9 Pixel = 45/9 = 5.0
        for val in result {
            assert!((val - 5.0).abs() < 0.1, "Box-Filter Test fehlgeschlagen");
        }
    }
    
    #[test]
    fn test_guided_filter() {
        // Einfaches Testbild
        let img = vec![
            0.5, 0.5, 0.5,
            0.5, 0.5, 0.5,
            0.5, 0.5, 0.5
        ];
        
        let result = guided_filter(&img, &img, 3, 3, 1, 0.01);
        
        // Bei konstantem Bild sollte das Ergebnis gleich dem Input sein
        for (i, &val) in result.iter().enumerate() {
            assert!((val - img[i]).abs() < 0.01, "Guided Filter Test fehlgeschlagen");
        }
    }
    
    #[test]
    fn test_rgb_to_luminance() {
        // Test mit Weiß (1,1,1)
        assert!((rgb_to_luminance(1.0, 1.0, 1.0) - 1.0).abs() < 0.001);
        
        // Test mit Schwarz (0,0,0)
        assert!((rgb_to_luminance(0.0, 0.0, 0.0) - 0.0).abs() < 0.001);
        
        // Test mit Rot (1,0,0)
        let lum = rgb_to_luminance(1.0, 0.0, 0.0);
        assert!((lum - 0.299).abs() < 0.001);
    }
}
