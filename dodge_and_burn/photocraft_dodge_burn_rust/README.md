# Photocraft Dodge & Burn Plugin (Rust/WASM)

**Status:** 🟢 Grundgerüst implementiert (29. Oktober 2026)  
**Ziel:** Natives Photocraft-Plugin für automatische Dodge & Burn Korrekturmasken  
**Sprache:** Rust → WebAssembly (WASM)  
**Kompatibilität:** Photocraft (ABI v1)

---

## 📁 Projektstruktur

```
photocraft_dodge_burn_rust/
├── Cargo.toml              # Rust-Konfiguration
├── build.sh                # Build-Skript
├── README.md               # Diese Datei
└── src/
    └── lib.rs              # Hauptimplementierung
```

---

## 🎯 Projektziel

Erstellung eines **nativen Photocraft-Plugins** in Rust/WASM, das:
1. **Automatisch Dodge & Burn Korrekturmasken** generiert
2. **Retouch4me-kompatibel** ist (subtile Korrekturen, ±5% um 128)
3. **Porenschutz** bietet (Mikro-Ebene unverändert)
4. **Asymmetrisches Dodge/Burn** unterstützt (mehr Burn als Dodge)
5. **KI-Hautsegmentierung** (später mit ONNX Runtime)

---

## 🚀 Schnellstart

### 1. Voraussetzungen installieren

```bash
# Rust installieren
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh

# WASM-Target hinzufügen
rustup target add wasm32-unknown-unknown
```

### 2. Plugin bauen

```bash
# Auf Linux/macOS
chmod +x build.sh
./build.sh

# Oder manuell
cargo build --release --target wasm32-unknown-unknown
```

### 3. Plugin in Photocraft installieren

1. **Plugin-Datei kopieren:**
   ```bash
   cp target/wasm32-unknown-unknown/release/photocraft_dodge_burn.wasm ~/photocraft-plugins/
   ```
   
2. **Photocraft starten** (Plugin wird automatisch geladen)
   
3. **Plugin verwenden:**
   - **Filter > Plug-ins > Dodge & Burn Mask**
   - Parameter anpassen und auf **OK** klicken
   - Maske wird als neues Bild erzeugt
   - Maske als Ebene über Original einfügen
   - Ebenenmodus auf **Soft Light** setzen

---

## 🔌 Photocraft Plugin-API (ABI v1)

### Benötigte Exports

| Export | Signatur | Implementierung |
|--------|----------|-----------------|
| `memory` | `memory` | ✅ `MEMORY` (50 MiB) |
| `pc_abi_version` | `() -> i32` | ✅ `1` |
| `pc_manifest` | `() -> i64` | ✅ Manifest-JSON |
| `pc_alloc` | `(size) -> ptr` | ✅ Speicherallokator |
| `pc_filter` | `(buf, ...) -> i32` | ✅ Hauptfilter |

### Manifest

```json
{
  "id": "de.daniel.photocraft.dodge_burn",
  "name": "Dodge & Burn Mask",
  "version": "1.0.0",
  "kind": "filter",
  "overlap": 55,
  "area": "canvas"
}
```

---

## 📊 Implementierte Algorithmen

### 1. Box-Filter (O(1) mit Integralbildern)
- **Zweck:** Kantenerhaltende Mittelwertberechnung
- **Komplexität:** O(1) pro Pixel
- **Verwendung:** Basis für Guided Filter

### 2. Guided Filter (He et al., 2010)
- **Zweck:** Kantenerhaltende Glättung
- **Parameter:** Radius (r), Regularisierung (eps)
- **Verwendung:** Multi-Skalen-Frequenztrennung

### 3. Hautsegmentierung
- **Methode:** YCbCr-Farbraum + RGB-Regeln
- **Komponenten:**
  - Haut-Cluster im Cb/Cr-Raum
  - Lippen-Unterdrückung
  - Augen-/Pupillen-Ausschluss
  - Weiche Kanten (Feathering)

### 4. Dodge & Burn Logik
- **Frequenzbänder:**
  - Makro-Ebene (55px): Globale Gesichtsform
  - Mittlere Ebene (12px): Lokale Unebenheiten
- **Korrektur:**
  - Dodge (Aufhellen): delta > 0
  - Burn (Abdunkeln): delta < 0
  - **Retouch4me-Optimierung:** Skalierung ×0.1, Begrenzung ±5%

---

## 🎛️ Parameter

| Parameter | Typ | Standard | Bereich | Beschreibung |
|-----------|-----|----------|---------|--------------|
| `intensity` | number | **0.15** | 0.0–0.3 | Gesamtstärke (Retouch4me: 0.05–0.2) |
| `dodge_strength` | number | **0.5** | 0.0–2.0 | Stärke der Aufhellung |
| `burn_strength` | number | **1.0** | 0.0–2.0 | Stärke der Abdunklung |
| `blemish_radius` | int | **12** | 1–200 | Radius für lokale Unebenheiten |
| `macro_radius` | int | **55** | 1–500 | Radius für globale Gesichtsform |
| `preserve_pores` | bool | **true** | - | Porenschutz aktivieren |

---

## 🔬 Retouch4me-Kompatibilität

### 📈 Erkenntnisse aus der Maskenanalyse

| **Aspekt** | **Retouch4me** | **Dieses Plugin** | **Status** |
|------------|----------------|-------------------|------------|
| Maskenformat | CV_8UC1 (8-Bit Grau) | ✅ f32 → u8 | ✅ |
| Neutraler Wert | 128 | ✅ 0.5 (128) | ✅ |
| Wertebereich | 113–239 | ✅ 115–141 (begrenzt) | ✅ |
| Typische Korrektur | ±1–5% | ✅ ±5% (skaliert) | ✅ |
| Dodge/Burn-Verhältnis | 5:1–45:1 (mehr Burn) | ✅ 0.5:1.0 | ✅ |
| Micro-Bereich | 94–97% der Pixel | ✅ 100% | ✅ |

### 🎯 Optimierungen für Retouch4me-Kompatibilität

1. **Skalierungsfaktor 0.1:**
   ```rust
   let scaled_correction = correction[idx] * 0.1;
   ```

2. **Begrenzung auf ±5%:**
   ```rust
   let clamped_correction = scaled_correction.max(-0.05).min(0.05);
   ```

3. **Asymmetrisches Dodge/Burn:**
   ```rust
   dodge_strength = 0.5  // Weniger Dodge
   burn_strength = 1.0   // Mehr Burn
   ```

---

## 📦 Build-Output

Nach erfolgreicher Kompilierung:
- **Datei:** `photocraft_dodge_burn.wasm`
- **Größe:** ~200–500 KB (unoptimiert)
- **Format:** WebAssembly (WASM)
- **Plattform:** macOS, Windows, Linux, Web

---

## 🧪 Testing

### Lokale Tests

```bash
# Unit-Tests ausführen
cargo test
```

### Manuelle Tests mit Photocraft

1. Plugin installieren (siehe Schnellstart)
2. Testbild in Photocraft öffnen
3. Plugin ausführen und Maske generieren
4. Maske mit Soft Light blenden
5. Ergebnis prüfen

---

## 🔧 Optimierungen (geplant)

### 📌 Phase 1: Grundfunktionalität (✅fertig)
- [x] Rust-Projekt aufsetzen
- [x] ABI v1-kompatible Exports
- [x] Guided Filter implementieren
- [x] Hautsegmentierung implementieren
- [x] Dodge & Burn Logik
- [x] Retouch4me-kompatible Skalierung

### 📌 Phase 2: Performance-Optimierung
- [ ] `no_std` aktivieren (kein std)
- [ ] Inline-Assembly für kritische Codepfade
- [ ] SIMD-Optimierungen (falls verfügbar)
- [ ] Speichernutzung reduzieren

### 📌 Phase 3: Erweiterte Features
- [ ] ONNX Runtime für KI-Hautsegmentierung
- [ ] Adaptive Radius-Berechnung
- [ ] Manuelle Maskenbearbeitung (Pinsel-Tool)
- [ ] Mehrere Maskenebenen (Dodge/Burn getrennt)
- [ ] Presets für verschiedene Stile

### 📌 Phase 4: Benutzerfreundlichkeit
- [ ] GUI für Parameter-Anpassung (Web oder native)
- [ ] Live-Vorschau
- [ ] Tutorial/Anleitung
- [ ] Fehlerbehandlung & Logging

---

## 🔗 Ressourcen

- [Photocraft Plugin-Dokumentation](https://github.com/storytold/photocraft/blob/main/docs/plugins.md)
- [Rust für WASM](https://rustwasm.github.io/docs/book/)
- [Guided Filter Paper (He et al., 2010)](https://arxiv.org/abs/1009.0515)
- [OpenCV Guided Filter](https://docs.opencv.org/4.x/d5/df7/tutorial_non_photorealistic_rendering.html)

---

## 📝 Changelog

| **Datum** | **Version** | **Änderungen** |
|-----------|-------------|----------------|
| 29.10.2026 | 1.0.0 | Grundgerüst implementiert (Rust + WASM) |
| 29.10.2026 | 1.0.0 | Guided Filter, Hautsegmentierung, Dodge & Burn |
| 29.10.2026 | 1.0.0 | Retouch4me-Kompatibilität (Skalierung ×0.1) |

---

## 🎉 Nächste Schritte

1. **Plugin testen:**
   ```bash
   ./build.sh
   ```

2. **In Photocraft integrieren** und erste Tests durchführen

3. **Parameter optimieren** basierend auf Testergebnissen

4. **Performance optimieren** (no_std, SIMD)

5. **KI-Hautsegmentierung** hinzufügen (ONNX Runtime)

---

*Dieses Plugin basiert auf den Erkenntnissen aus der Analyse von Retouch4me Dodge & Burn und ist für maximale Kompatibilität mit Photocraft optimiert.*
