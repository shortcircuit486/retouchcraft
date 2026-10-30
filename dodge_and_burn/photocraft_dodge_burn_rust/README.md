# Photocraft Dodge & Burn Plugin (Rust/WASM)

**Status:** v1.7.0 — produktiv getestet in Photocraft (Linux)
**Ziel:** Natives Photocraft-Plugin (ABI v1), das eine 50%-Neutralgrau-Korrekturmaske
für Dodge & Burn auf Hautbereichen erzeugt.
**Sprache:** Rust → WebAssembly (`wasm32-unknown-unknown`, keine externen Dependencies)

---

## 1. Projektstruktur

```
photocraft_dodge_burn_rust/
├── Cargo.toml              # cdylib + rlib (rlib nötig für Integrationstests)
├── build.sh                # cargo build --release --target wasm32-unknown-unknown
│                           # und Kopie nach ../photocraft_dodge_burn/photocraft_dodge_burn.wasm
├── src/lib.rs              # komplette Implementierung + Unit-Tests
└── tests/test_plugin.rs    # Integrationstests + Benchmarks (#[ignore])
```

## 2. Schnellstart

```bash
./build.sh                # baut target/.../photocraft_dodge_burn.wasm und kopiert es
                          # nach ../photocraft_dodge_burn/photocraft_dodge_burn.wasm
cargo test                # 8 Tests (Unit + Integration)
cargo test --test test_plugin --release -- --ignored --nocapture   # Benchmarks
```

**Installation in Photocraft:** WASM-Datei in den Plugin-Ordner kopieren
(Edit › Preferences › Plug-ins, oder `~/photocraft-plugins/`), Photocraft neu starten.
Danach: **Filter › Plug-ins › Dodge & Burn Mask**.

**Wichtig zum Workflow (ABI v1):** `plugin.run` arbeitet **in-place auf der aktiven
Ebene** — es wird keine neue Ebene angelegt und kein Mischmodus gesetzt. Empfohlener
Ablauf:

1. Ebene duplizieren (z. B. „Background copy")
2. Plugin auf der Kopie ausführen → die Kopie wird zur Graumaske
3. Kopie über das Original legen, Mischmodus **Soft Light (Weiches Licht)**, Deckkraft 60–85 %

## 3. Photocraft Plugin-API (ABI v1) — die wichtigsten Punkte

Referenz: <https://github.com/storytold/photocraft/blob/main/docs/plugins.md>

| Aspekt | Verhalten des Hosts |
|---|---|
| Puffer | `width × height × channels` little-endian **f32, normalisiert 0–1**, interleaved; **Alpha = letzter Kanal** (format-Bit 8, straight alpha) |
| Ausführung | in-place auf der aktiven Ebene; danach Blend mit Selektion: `out = orig + (filtered − orig) × coverage` |
| Bänder | Ebene wird in Kacheln (~4 MiB f32) zerlegt; jede Kachel bekommt `overlap` Kontextpixel (echte Nachbarpixel, am Canvas-Rand edge-repliziert); nur der Innenbereich des Ergebnisses wird übernommen |
| `_image` in params | enthält `x, y` (Dokumentposition des Puffers inkl. Ränder), `width, height, overlap, canvasWidth, canvasHeight, ...` |
| Budgets | 60 s Wallclock pro Run; 50 M + 4000/Sample Instruktionen pro Aufruf; 512 MiB lineares Memory pro Instanz; NaN/Inf-Ausgaben behalten den Originalwert |
| Vorschau | läuft auf reduzierter Kopie; `px`-Parameter werden durch die Reduktion geteilt |

## 4. Parameter (Manifest v1.7.0)

| Parameter | Typ | Min | Max | Default | Bedeutung |
|---|---|---|---|---|---|
| `intensity` | number | 0 | 2 | **0.8** | Gesamtstärke |
| `dodge_strength` | number | 0 | 2 | **1.0** | Gewichtung Aufhellen |
| `burn_strength` | number | 0 | 2 | **1.0** | Gewichtung Abdunkeln |
| `blemish_radius` | int (px) | 1 | 200 | **12** | mittlere Frequenz (Blemish-Band) |
| `macro_radius` | int (px) | 1 | 500 | **55** | Makro-Frequenz (Gesichtsform) |
| `preserve_pores` | bool | – | – | **true** | wird geparst, aktuell ohne Effekt (siehe Limitationen) |

Manifest: `overlap: 60` (≥ `macro_radius`-Default, damit Kachel-Ränder korrekte
Kontextpixel haben), `area: "canvas"`, `kind: "filter"`.

## 5. Algorithmus (v1.7.0)

1. **Adaptives Zellraster (Performance):** Photocraft interpretiert WASM über wasmi
   (~10–100× langsamer als nativ). Der Guided Filter über jedes Pixel eines
   100-MP-Dokuments sprengt das 60-s-Budget. Die Korrektur ist bandbegrenzt
   (alle Radien ≥ `blemish_radius`), daher wird auf einem dokumentglobalen
   Zellraster gerechnet (Ziel ~2 M Zellen; `scale = ceil(sqrt(canvasPx / 2M))`,
   gedeckelt auf `blemish_radius/3`, damit Blemish-Spots mit mehreren Zellen
   aufgeloest werden) und das Ergebnis **bilinear hochgerechnet**.
   Ausrichtung am Dokumentraster via `_image.x/y` (nahtfrei über Kachelgrenzen).
   Kachel-Ränder (aus `_image.overlap`) bekommen konstant 0.5. Bei `scale == 1`
   (kleine Dokumente) ist das Verhalten bit-identisch zur pixel-weisen Rechnung.
2. **Guided Self-Filter** (p == I, 4 statt 6 Box-Durchläufe, identische Werte):
   Makro-Ebene (r = `macro_radius/scale`, eps 0.03) und mittlere Ebene
   (r = `blemish_radius/scale`, eps 0.006) aus der Zell-Luminanz.
3. **Kontinuierliche Basiskurve (keine Deadzone!):**
   `base = 0.5 · |delta|^0.85 · sign(delta)` — feines, überall vorhandenes
   Korrekturfeld (Retouch4me-Charakter: ~47 % der Pixel mit 0,5–5 u8-Stufen).
4. **Blemish-Spot-Schicht (additiv, nur Dodge):** Retouch4me hellt gerötete,
   lokal dunkle Hautstellen gezielt auf. Kalibriert per Regression gegen echte
   Retouch4me-Masken:
   `score = (30·(r−g) + 21·max(mid−lum, 0) − 10·max(macro−lum, 0)) / 255`
   `spot = 2.2 · max(score − 0.020, 0)^0.8`
   (Röte ist der stärkste Prädiktor; breite dunkle Schattierung zählt negativ.)
5. **Hautmaske:** YCbCr-Cluster + RGB-Regel + Lippen-/Augen-Suppression; Korrektur
   wird mit der Hautwahrscheinlichkeit multipliziert.
6. **Alpha bleibt unangetastet** (format-Bit 8): Die Maske wird nur in die
   Farbkanäle geschrieben. (v1.2-Bug: Alpha wurde überschrieben → halbtransparente
   Schleier-Ebene → „flache Optik" bei Ebenen mit Alphakanal.)
7. **Ausgabe:** `mask = 0.5 + clamp(correction, ±0.3)` in alle Farbkanäle.

## 6. Messungen (Referenz: echte Retouch4me-Maske, Bild pexels-salim-serdar…, 40 %-Raster)

| Metrik | Retouch4me | v1.3 | v1.4 (Deadzone) | **v1.5** |
|---|---|---|---|---|
| feine Bande (0,5–5 u8) | 47 % | 51 % | 2 % | **50 %** |
| aktive 32-px-Kacheln | 71 % | 68 % | 20 % | **72 %** |
| Feinstruktur (HP-std) | 0,56 | 0,34 | 0,77 | **0,44** |
| strukturelle Korrelation | 1,0 | 0,332 | 0,175 | **0,487** |
| p90 / p99 / max | 4,5 / 20,5 / 55 | 4,9 / 11,1 / 21 | – | **6,2 / 9,8 / 19** |

**Performance:** 2,65 ms pro 632×632-RGBA-Kachel (nativ, release) → 8736×11648
(388 Kacheln) ≈ 1 s nativ; WASM-Overhead hält den Lauf im 60-s-Budget.
Instanzspeicher ~1 MB (v1.2 brauchte ~50 MB) → viele parallele Kacheln möglich.

## 7. Tests

```bash
cargo test                 # 4 Unit- (lib.rs) + 4 Integrationstests (tests/test_plugin.rs)
```

Integrationstests: ABI-Version, Basislauf, **Alpha-Erhaltung bei RGBA**,
Downsampling-Pfad (Groß-Canvas via `_image`, Ränder = 0.5, Innen = Korrektur).
Benchmarks (ignoriert, manuell): `cargo test --test test_plugin --release -- --ignored --nocapture`.

## 8. Changelog

| Version | Änderungen |
|---|---|
| 1.0.0 | Grundgerüst: Box/Guided Filter, Hautmaske, D&B, ×0.1-Dämpfung |
| 1.1.0 | Bounds-Checking, Fehlerbehandlung |
| 1.2.0 | Alpha-Kanal wird nicht mehr überschrieben; overlap 20→60; Box-Filter-Ränder teilen durch tatsächliche Fensterfläche |
| 1.3.0 | Adaptives dokumentglobales Zellraster + bilineares Hochrechnen (Zeitbudget-Fix); Guided-Self-Filter; Tests + Benchmarks |
| 1.4.0 | Deadzone+Gamma-Kurve — **Regression** (löscht feine Korrekturen, 88 % neutral); nicht verwenden |
| **1.5.0** | S1–S3: Deadzone entfernt, kontinuierliche Basiskurve 0.5·\|Δ\|^0.85, röte-basierte Blemish-Spot-Schicht; Korrelation zur R4m-Referenzmaske 0,487 |
| **1.6.0** | S4: Zellraster feiner — scale-Cap `blemish_radius/3` (Vollbild s=8→4, HP-std 0,20→~0,36; Kachel-Bench 4,85 ms, Zeitbudget weiterhin eingehalten) |
| **1.7.0** | S5: Burn-Asymmetrie — Burn-Cap −9 u8 (R4m-Burn-Tail) + Burn-Floor −2 u8 auf Haut; 125–129-Anteil 60→65 %, Korrelation 0,492 |

## 9. Limitationen / Offen

- `preserve_pores` wird geparst, aber nicht verwendet (Porenerhalt ergibt sich
  implizit aus der Bandbegrenzung der Maske).
- `overlap` ist statisch im Manifest; `macro_radius` > 60 im Dialog kann bei sehr
  großen Dokumenten Kachel-Nähte erzeugen.
- Der extreme Dodge-Schwanz von Retouch4me (Maske bis +55 u8) ist mit
  Signalverarbeitungs-Merkmalen nicht exakt lokalisierbar (Pareto-Analyse);
  dafür wäre ein gelernter Detektor (z. B. ONNX) nötig.
- Die Spot-Kalibrierung basiert auf einer Referenzmaske; auf anderen Motiven
  ist die Übereinstimmung geringer (Kreuzvalidierung: 0,03–0,39), schadet aber
  nicht.
- Alte Empfehlung (v1.0-README: „×0.1 dämpfen, ±5 % clamp, dodge 0.5") basierte
  auf der Pixelzahl der R4m-Masken, nicht auf deren Wirkung — die Energie liegt
  zu 77 % im Dodge. Nicht mehr anwenden.

## 10. Ressourcen

- [Photocraft Plugin-Dokumentation (ABI v1)](https://github.com/storytold/photocraft/blob/main/docs/plugins.md)
- [Guided Filter (He et al., 2010)](https://arxiv.org/abs/1009.0515)
- Soft-Light-Blend-Formel: siehe Adobe Photoshop SDK / W3C Compositing
