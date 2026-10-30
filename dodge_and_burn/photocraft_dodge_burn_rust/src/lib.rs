// Photocraft Dodge & Burn Plugin (WASM)
// =====================================
// ABI v1-kompatibles Plugin für Photocraft
// Erzeugt 50% Neutralgrau-Masken für Dodge & Burn Korrekturen
//
// Autor: Daniel
// Datum: 30. Oktober 2026
// Version: 1.5.0 (S1-S3: kontinuierliche Basiskurve + Röte-basierte Blemish-Spots)

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
  "id": "de.daniel.photocraft.dodge_burn",
  "name": "Dodge & Burn Mask",
  "version": "1.5.0",
  "kind": "filter",
  "author": "Daniel",
  "description": "Generiert eine 50% Neutralgrau-Maske für Dodge & Burn Korrekturen. Fügen Sie die Maske als Ebene mit Soft-Light-Modus ein.",
  "params": {
    "intensity": {
      "type": "number",
      "min": 0.0,
      "max": 2.0,
      "default": 0.8,
      "unit": "none",
      "description": "Gesamtstärke der Korrektur"
    },
    "dodge_strength": {
      "type": "number",
      "min": 0.0,
      "max": 2.0,
      "default": 1.0,
      "unit": "none",
      "description": "Stärke der Aufhellung (Dodge)"
    },
    "burn_strength": {
      "type": "number",
      "min": 0.0,
      "max": 2.0,
      "default": 1.0,
      "unit": "none",
      "description": "Stärke der Abdunklung (Burn)"
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
  "overlap": 60,
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

/// Parst eine Zahl aus einem JSON-Objekt, das nach einem Marker (z.B. "_image")
/// beginnt. Gibt None zurück, wenn Marker oder Schlüssel fehlen oder der Wert
/// nicht lesbar ist (Aufrufer nutzen dann einen sicheren Fallback).
fn parse_num_after(json: &str, key: &str, marker: &str) -> Option<f64> {
    let marker_pos = json.find(marker)?;
    let search = format!("\"{}\":", key);
    let rel = json[marker_pos..].find(&search)?;
    let rest = json[marker_pos + rel + search.len()..].trim_start();
    let end = rest
        .find(|c: char| !(c.is_ascii_digit() || c == '.' || c == '-' || c == '+'))
        .unwrap_or(rest.len());
    rest[..end].parse::<f64>().ok()
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

    for y in 0..height {
        for x in 0..width {
            let x_i32 = x as i32;
            let y_i32 = y as i32;
            let x1 = (x_i32 - r_i32).max(0) as usize;
            let x2 = ((x_i32 + r_i32 + 1) as usize).min(width);
            let y1 = (y_i32 - r_i32).max(0) as usize;
            let y2 = ((y_i32 + r_i32 + 1) as usize).min(height);

            // Tatsächliche (am Rand geklemmte) Fenstergröße statt (2r+1)^2,
            // sonst werden Bildränder systematisch abgedunkelt.
            let area = ((x2 - x1) * (y2 - y1)) as f32;

            let sum = integral[y2 * (width + 1) + x2] 
                    + integral[y1 * (width + 1) + x1] 
                    - integral[y2 * (width + 1) + x1] 
                    - integral[y1 * (width + 1) + x2];
            
            output[y * width + x] = sum / area;
        }
    }
    
    output
}

/// Guided Filter (kantenerhaltend) nach He et al., Spezialfall p == I.
/// Führt dieselben Rechenschritte wie die allgemeine Variante mit p = I aus
/// (identische Ergebnisse), benötigt aber nur 4 statt 6 Box-Durchläufe.
fn guided_self_filter(img: &[f32], width: usize, height: usize, r: usize, eps: f32) -> Vec<f32> {
    // Schritt 1: Mittelwerte von I und I*I
    let mean_i = box_filter(img, width, height, r);

    let mut squared = vec![0.0f32; width * height];
    for idx in 0..width * height {
        squared[idx] = img[idx] * img[idx];
    }
    let corr_i = box_filter(&squared, width, height, r);

    // Schritt 2: Varianz und Filterkoeffizienten a und b (mit p == I ist
    // cov_ip == var_i und mean_p == mean_i)
    let n = width * height;
    let mut a = vec![0.0f32; n];
    let mut b = vec![0.0f32; n];
    for idx in 0..n {
        let var_i = (corr_i[idx] - mean_i[idx] * mean_i[idx]).max(0.0);
        let denominator = var_i + eps;
        // Vermeide NaN durch sichere Division
        if denominator > 1e-10 {
            a[idx] = var_i / denominator;
        }
        b[idx] = mean_i[idx] - a[idx] * mean_i[idx];
        // Ersetze NaN-Werte durch 0
        if a[idx].is_nan() { a[idx] = 0.0; }
        if b[idx].is_nan() { b[idx] = 0.0; }
    }

    // Schritt 3: Filtere a und b
    let mean_a = box_filter(&a, width, height, r);
    let mean_b = box_filter(&b, width, height, r);

    // Schritt 4: Berechne das gefilterte Bild
    let mut output = vec![0.0f32; width * height];
    for idx in 0..width * height {
        output[idx] = mean_a[idx] * img[idx] + mean_b[idx];
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
// KORREKTURKURVEN (kalibriert gegen echte Retouch4me-Masken)
// ============================================================================

// Basis-Kurve: kontinuierlich, KEINE Deadzone. Liefert das feine, überall
// vorhandene Korrekturfeld (Retouch4me: ~47% der Pixel mit 0.5-5 u8-Stufen).
const BASE_GAMMA: f32 = 0.85;
const BASE_GAIN: f32 = 0.5;

// Blemish-Spot-Schicht (additiv, nur Dodge): gerötete, lokal dunkle
// Hautstellen werden gezielt aufgehellt. Lineare Regression gegen die echte
// Retouch4me-Maske: Röte ist der stärkste Prädiktor, breite dunkle
// Schattierung (dunkel ggü. Makro-Ebene) ist KEIN Blemish (negatives Gewicht).
const SPOT_RED_WEIGHT: f32 = 30.0;         // u8 pro (r - g)
const SPOT_DARK_MID_WEIGHT: f32 = 21.0;    // u8, dunkel ggü. mittlerer Ebene
const SPOT_DARK_MACRO_WEIGHT: f32 = -10.0; // u8, dunkel ggü. Makro-Ebene
const SPOT_THRESHOLD: f32 = 0.020;         // Maskeneinheiten (u8/255)
const SPOT_GAMMA: f32 = 0.8;
const SPOT_GAIN: f32 = 2.2;

/// Nichtlineare Kurve mit optionaler Deadzone: unterhalb der Deadzone exakt
/// neutral, darüber gain * x^gamma. Die Basis nutzt Deadzone 0 (kontinuierlich),
/// die Spot-Schicht nutzt die Deadzone als Schwellwert.
#[inline]
fn correction_curve(x: f32, deadzone: f32, gamma: f32, gain: f32) -> f32 {
    if x <= deadzone {
        0.0
    } else {
        let m = x - deadzone;
        gain * m.powf(gamma)
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
    format: i32,
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
    let intensity = parse_f32_param(params_str, "intensity", 0.8);
    let dodge_strength = parse_f32_param(params_str, "dodge_strength", 1.0);
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
    
    // === 5. Zellraster bestimmen (Performance: Zeitbudget des Hosts) ===
    // Photocraft interpretiert WASM über wasmi (~10-100x langsamer als nativ);
    // der Guided Filter über jedes Pixel eines 100-Megapixel-Dokuments
    // überschreitet das 60s-Zeitbudget. Die Korrekturmaske ist bandbegrenzt
    // (Frequenzanteile nur bis ~1/blemish_radius), daher wird die Berechnung
    // auf ein gröberes Zellraster verlagert und das Ergebnis bilinear
    // hochgerechnet. Kleine Dokumente (scale == 1) rechnen wie zuvor pro Pixel.
    let canvas_w = parse_num_after(params_str, "canvasWidth", "_image")
        .map(|v| v.max(1.0) as usize)
        .unwrap_or(width_usize);
    let canvas_h = parse_num_after(params_str, "canvasHeight", "_image")
        .map(|v| v.max(1.0) as usize)
        .unwrap_or(height_usize);

    const TARGET_CELLS: usize = 2_000_000;
    let canvas_px = canvas_w.saturating_mul(canvas_h);
    let mut scale: usize = 1;
    if canvas_px > TARGET_CELLS {
        let f = canvas_px as f64 / TARGET_CELLS as f64;
        scale = f.sqrt().ceil() as usize;
    }
    // Radien müssen auf dem Zellraster darstellbar bleiben (>= 1 Zelle)
    let scale = scale.max(1).min(blemish_radius.max(1) as usize);

    // Zellraster dokumentglobal ausrichten (via _image.x/_image.y), damit
    // benachbarte Bänder dasselbe Raster nutzen und keine Naht-Artefakte
    // entstehen. Ohne _image: lokales Raster ab Puffer-Nullpunkt.
    let img_x = parse_num_after(params_str, "x", "_image")
        .map(|v| v as i32)
        .unwrap_or(0);
    let img_y = parse_num_after(params_str, "y", "_image")
        .map(|v| v as i32)
        .unwrap_or(0);
    let base_cx = img_x.div_euclid(scale as i32);
    let base_cy = img_y.div_euclid(scale as i32);
    let cells_x = ((img_x + width_usize as i32 - 1).div_euclid(scale as i32) - base_cx + 1)
        .max(1) as usize;
    let cells_y = ((img_y + height_usize as i32 - 1).div_euclid(scale as i32) - base_cy + 1)
        .max(1) as usize;

    // Zellspalte/-zeile je Pufferspalte/-reihe (einmal vorberechnen)
    let mut cell_col = vec![0usize; width_usize];
    for x in 0..width_usize {
        cell_col[x] = ((img_x + x as i32).div_euclid(scale as i32) - base_cx) as usize;
    }
    let mut cell_row = vec![0usize; height_usize];
    for y in 0..height_usize {
        cell_row[y] = ((img_y + y as i32).div_euclid(scale as i32) - base_cy) as usize;
    }

    let cell_w = cells_x;
    let cell_h = cells_y;
    let cell_count = cell_w * cell_h;

    // === 6. Zellen mitteln (Luminanz + Hautfarbe) ===
    // Bei großen Skalen genügt eine Teilmenge der Pixel pro Zelle.
    let stride = if scale > 1 { scale / 2 } else { 1 };
    let mut sum_r = vec![0.0f32; cell_count];
    let mut sum_g = vec![0.0f32; cell_count];
    let mut sum_b = vec![0.0f32; cell_count];
    let mut cell_cnt = vec![0.0f32; cell_count];
    let mut yy = 0;
    while yy < height_usize {
        let cy = cell_row[yy];
        let mut xx = 0;
        while xx < width_usize {
            let idx = (yy * width_usize + xx) * channels_usize;
            let c = cy * cell_w + cell_col[xx];
            if channels_usize >= 3 {
                sum_r[c] += pixels[idx];
                sum_g[c] += pixels[idx + 1];
                sum_b[c] += pixels[idx + 2];
            } else {
                sum_r[c] += pixels[idx];
                sum_g[c] += pixels[idx];
                sum_b[c] += pixels[idx];
            }
            cell_cnt[c] += 1.0;
            xx += stride;
        }
        yy += stride;
    }

    let mut lum = vec![0.0f32; cell_count];
    let mut redness = vec![0.0f32; cell_count];
    let mut skin_mask = vec![1.0f32; cell_count];
    for c in 0..cell_count {
        let n = if cell_cnt[c] > 0.0 { cell_cnt[c] } else { 1.0 };
        let r = sum_r[c] / n;
        let g = sum_g[c] / n;
        let b = sum_b[c] / n;
        lum[c] = rgb_to_luminance(r, g, b);
        redness[c] = r - g;
        if channels_usize >= 3 {
            skin_mask[c] = compute_skin_probability(r, g, b);
        }
    }

    // === 7. Multi-Skalen-Frequenztrennung (auf dem Zellraster) ===
    let blemish_cells =
        ((blemish_radius as f32) / (scale as f32)).round().max(1.0) as usize;
    let macro_cells =
        (((macro_radius as f32) / (scale as f32)).round() as usize).max(blemish_cells + 1);

    // Makro-Ebene: Grobe Gesichtsform
    let macro_lum = guided_self_filter(&lum, cell_w, cell_h, macro_cells, 0.03);

    // Mittlere Ebene: Lokale Unebenheiten
    let mid_lum = guided_self_filter(&lum, cell_w, cell_h, blemish_cells, 0.006);

    // === 8./9. Dodge & Burn Korrektur je Zelle, maskiert mit Haut ===
    let mut mask_cell = vec![0.5f32; cell_count];
    for c in 0..cell_count {
        let delta = macro_lum[c] - mid_lum[c];

        // Kontinuierliche Basis (keine Deadzone): feines, überall
        // vorhandenes Korrekturfeld
        let dodge_part =
            correction_curve(delta.max(0.0), 0.0, BASE_GAMMA, BASE_GAIN) * dodge_strength;
        let burn_part =
            -correction_curve((-delta).max(0.0), 0.0, BASE_GAMMA, BASE_GAIN) * burn_strength;

        // Blemish-Spot-Schicht (additiv, nur Dodge): gerötete, lokal
        // dunkle Hautstellen gezielt aufhellen (Retouch4me-Hotspots)
        let spot_score = (SPOT_RED_WEIGHT * redness[c]
            + SPOT_DARK_MID_WEIGHT * (mid_lum[c] - lum[c]).max(0.0)
            + SPOT_DARK_MACRO_WEIGHT * (macro_lum[c] - lum[c]).max(0.0))
            / 255.0;
        let spot =
            correction_curve(spot_score, SPOT_THRESHOLD, SPOT_GAMMA, SPOT_GAIN) * dodge_strength;

        let correction = (dodge_part + burn_part + spot) * intensity * skin_mask[c];

        // Begrenzung auf ±0.3 (30% um 0.5 → 0.2 bis 0.8)
        let clamped = correction.max(-0.3).min(0.3);
        mask_cell[c] = 0.5 + clamped;
    }

    // === 10. Bilinear hochrechnen und in die Farbkanäle schreiben ===
    // format-Bit 8 (ABI v1): der letzte Kanal ist Alpha. Alpha bleibt unangetastet,
    // sonst wird die Ebene halbtransparent und das Bild wirkt beim Blenden "flach".
    let has_alpha = (format >> 8) & 1 == 1;
    let color_channels = if has_alpha && channels_usize >= 2 {
        channels_usize - 1
    } else {
        channels_usize
    };

    // Innenbereich des Bandes (ohne Kontext-Ränder) aus _image bestimmen.
    // Der Host verwirft die Randpixel nach dem Filtern; sie bekommen
    // konstant 50% Grau statt der teuren Interpolation.
    let (in_x0, in_x1, in_y0, in_y1) = {
        let iw = parse_num_after(params_str, "width", "_image").map(|v| v.max(0.0) as usize);
        let ih = parse_num_after(params_str, "height", "_image").map(|v| v.max(0.0) as usize);
        let ovl = parse_num_after(params_str, "overlap", "_image").map(|v| v.max(0.0) as usize);
        match (iw, ih, ovl) {
            (Some(iw), Some(ih), Some(ovl))
                if ovl * 2 <= width_usize && ovl * 2 <= height_usize =>
            {
                if iw == width_usize && ih == height_usize {
                    (ovl, width_usize - ovl, ovl, height_usize - ovl)
                } else if iw + 2 * ovl == width_usize && ih + 2 * ovl == height_usize {
                    (ovl, ovl + iw, ovl, ovl + ih)
                } else {
                    (0, width_usize, 0, height_usize)
                }
            }
            _ => (0, width_usize, 0, height_usize),
        }
    };

    // x-Interpolationstabellen einmal vorberechnen (dokumentglobales Raster)
    let mut cx0 = vec![0usize; width_usize];
    let mut cx1 = vec![0usize; width_usize];
    let mut cfx = vec![0.0f32; width_usize];
    for x in 0..width_usize {
        let gx = (img_x as f32 + x as f32 + 0.5) / (scale as f32) - 0.5
            - base_cx as f32;
        let gx = gx.max(0.0).min((cell_w - 1) as f32);
        let x0 = gx.floor() as usize;
        cx0[x] = x0;
        cx1[x] = (x0 + 1).min(cell_w - 1);
        cfx[x] = gx - x0 as f32;
    }

    for y in 0..height_usize {
        let gy = (img_y as f32 + y as f32 + 0.5) / (scale as f32) - 0.5
            - base_cy as f32;
        let gy = gy.max(0.0).min((cell_h - 1) as f32);
        let y0 = gy.floor() as usize;
        let y1 = (y0 + 1).min(cell_h - 1);
        let fy = gy - y0 as f32;
        let row0 = y0 * cell_w;
        let row1 = y1 * cell_w;
        let inner_row = y >= in_y0 && y < in_y1;

        let mut pixel_idx = y * width_usize * channels_usize;
        for x in 0..width_usize {
            // 50% Neutralgrau + Korrektur (bilinear aus dem Zellraster)
            let mask_value = if inner_row && x >= in_x0 && x < in_x1 {
                let a0 = mask_cell[row0 + cx0[x]];
                let b0 = mask_cell[row0 + cx1[x]];
                let a1 = mask_cell[row1 + cx0[x]];
                let b1 = mask_cell[row1 + cx1[x]];
                let top = a0 + (b0 - a0) * cfx[x];
                let bot = a1 + (b1 - a1) * cfx[x];
                top + (bot - top) * fy
            } else {
                0.5
            };

            // Schreibe den Maskenwert nur in die Farbkanäle
            for c in 0..color_channels {
                pixels[pixel_idx + c] = mask_value;
            }
            pixel_idx += channels_usize;
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
        
        // Innenpixel: Fenster umfasst alle 9 Pixel → 45/9 = 5.0
        assert!((result[4] - 5.0).abs() < 1e-6, "Box-Filter Test fehlgeschlagen");
        // Eckpixel: Fenster auf 2x2 geklemmt → (1+2+4+5)/4 = 3.0
        assert!((result[0] - 3.0).abs() < 1e-6, "Box-Filter Test fehlgeschlagen");
    }
    
    #[test]
    fn test_guided_filter() {
        // Einfaches Testbild
        let img = vec![
            0.5, 0.5, 0.5,
            0.5, 0.5, 0.5,
            0.5, 0.5, 0.5
        ];
        
        let result = guided_self_filter(&img, 3, 3, 1, 0.01);
        
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

    #[test]
    fn test_correction_curve() {
        // Unterhalb der Deadzone: exakt neutral
        assert_eq!(correction_curve(0.0, 0.025, 0.65, 1.2), 0.0);
        assert_eq!(correction_curve(0.025, 0.025, 0.65, 1.2), 0.0);

        // Monotonie oberhalb der Deadzone
        let a = correction_curve(0.05, 0.025, 0.65, 1.2);
        let b = correction_curve(0.15, 0.025, 0.65, 1.2);
        assert!(a > 0.0 && b > a);

        // Gamma < 1: 40-facher Abstand über der Deadzone ergibt 40^0.65 ≈ 11x
        let small = correction_curve(0.03, 0.025, 0.65, 1.2);  // m = 0.005
        let big = correction_curve(0.225, 0.025, 0.65, 1.2);  // m = 0.200
        assert!(big > small * 10.5 && big < small * 11.5);
    }
}
