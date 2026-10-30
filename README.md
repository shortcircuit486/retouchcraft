# RetouchCraft

Open-Source-Retuschier-Plugins für den Bildeditor [Photocraft](https://github.com/storytold/photocraft) —
umgesetzt als **WebAssembly-Plugin (ABI v1)** in Rust. Photocraft hostet ausschließlich sandboxed
WASM-Plugins; native C++-/FFI-Erweiterungen sind nicht vorgesehen.

**Fokus:** deterministische, reproduzierbare Algorithmen für hochwertiges Porträt-Retusching —
präzise Tonwertsteuerung, Erhalt der Hautstruktur (Poren), keine Farbverschiebungen.

---

## Aktueller Stand

| Plugin | Status |
|---|---|
| **Dodge & Burn** (`RetouchCraft.DodgeBurn`) | **v1.7.0** — gebaut, 9 Tests grün, in Photocraft getestet |
| Blemish Removal | Idee / Roadmap |
| Face Contouring | Idee / Roadmap |

## Projektstruktur

```
dodge_and_burn/
├── photocraft_dodge_burn.wasm          # aktuelles Plugin-Build (v1.7.0)
├── photocraft_dodge_burn_rust/         # Rust-Quelle, Tests, Doku
│   ├── src/lib.rs                      # Implementierung + Unit-Tests
│   ├── tests/test_plugin.rs            # Integrationstests + Benchmarks
│   └── README.md                       # Detail-Doku (Algorithmus, Metriken, Changelog)
└── README.md                           # ursprünglicher Prototyp-Text (Python-Pipeline)
```

## Architektur

- **Plugin = eine `.wasm`-Datei**, Rust (`wasm32-unknown-unknown`), keine externen Crates
- **ABI v1:** Exporte `pc_abi_version`, `pc_manifest`, `pc_alloc`, `pc_filter` — **keine Imports**
  (kein WASI, kein Netzwerk, kein Dateizugriff; sandboxed via wasmi)
- **Pixel-Puffer:** f32, normalisiert 0–1, interleaved, zeilenweise; Alpha = letzter Kanal (format-Bit 8)
- `pc_filter` arbeitet **in-place auf der aktiven Ebene** (Undo macht der Host);
  Ebenen-/Soft-Light-Workflow liegt beim Anwender
- Der Host zerlegt Ebenen in Kacheln (~4 MiB f32) mit Overlap und rechnet die Kacheln parallel
- Details: [Photocraft Plugin-ABI v1](https://github.com/storytold/photocraft/blob/main/docs/plugins.md)

## Dodge & Burn (v1.7.0)

Erzeugt eine **50-%-Neutralgrau-Korrekturmaske** für den Mischmodus Soft Light:

1. adaptives, dokumentglobales Zellraster (~2 M Zellen, bilineares Hochrechnen)
2. Guided Filter (p = I) für Makro- und Blemish-Frequenzbänder
3. kontinuierliche Basiskurve `0,5 · |Δ|^0,85`
4. röte-basierte Blemish-Spot-Schicht (additiv, nur Dodge)
5. Hautmaske (YCbCr + RGB-Regeln); Burn-Cap −9 u8 + Burn-Floor −2 u8 (Retouch4me-Profil)
6. Alpha-Kanal der Ebene bleibt unangetastet

**Parameter (Manifest):**

| Parameter | Bereich | Default | Bedeutung |
|---|---|---|---|
| `intensity` | 0–2 | 0,8 | Gesamtstärke |
| `dodge_strength` | 0–2 | 1,0 | Gewichtung Aufhellen |
| `burn_strength` | 0–2 | 1,0 | Gewichtung Abdunkeln |
| `blemish_radius` | 1–200 px | 12 | Blemish-Band |
| `macro_radius` | 1–500 px | 55 | Makro-Frequenz |
| `preserve_pores` | bool | true | geparst, aktuell ohne Effekt |

## Build & Test

Voraussetzungen: Rust (rustup) und `rustup target add wasm32-unknown-unknown`

```bash
cd dodge_and_burn/photocraft_dodge_burn_rust
./build.sh        # baut ../photocraft_dodge_burn.wasm
cargo test        # 9 Tests (Unit + Integration + Maskenprofil)
```

## Installation & Anwendung in Photocraft

1. `dodge_and_burn/photocraft_dodge_burn.wasm` in den Plugin-Ordner kopieren
   (Edit › Preferences › Plug-ins), Photocraft neu starten
2. Bild öffnen → Ebene duplizieren → **Filter › Plug-ins › Dodge & Burn Mask** auf der Kopie ausführen
3. Kopie über das Original legen → Mischmodus **Soft Light (Weiches Licht)** → Deckkraft 60–85 %

## Performance (realistisch)

- Der Host interpretiert WASM ~10–100× langsamer als nativ; Budget: **60 s pro Lauf**
- Deshalb rechnet das Plugin auf dem Zellraster statt pro Pixel:
  8736 × 11648 px ≈ 1,9 s nativ; Bench: 4,85 ms pro 632×632-RGBA-Kachel (Release, nativ)
- ABI v1 erlaubt kein SIMD; die Parallelisierung über Kacheln übernimmt der Host

## Roadmap

- **Blemish Removal:** frequenzbasiertes, porenerhaltendes Patch-Healing als WASM-Modul
- **Face Contouring:** Oberflächennormalen aus 2D-Luminanz + Lichtsimulation (ambitioniert)
- Farbgenauigkeits-Testsuite; weitere Referenzmasken (Kalibrierung bisher single-image)

## Lizenz

MIT — siehe [LICENSE](LICENSE)

## Doku

Ausführliche Doku (Algorithmus, Parameter, Metriken vs. Retouch4me, Changelog):
[dodge_and_burn/photocraft_dodge_burn_rust/README.md](dodge_and_burn/photocraft_dodge_burn_rust/README.md)
