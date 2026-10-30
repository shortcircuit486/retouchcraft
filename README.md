# retouchcraft

Dodge & Burn für den Open-Source-Bildeditor [Photocraft](https://github.com/storytold/photocraft):
ein WASM-Plugin (ABI v1), das eine 50-%-Neutralgrau-Korrekturmaske für Porträts erzeugt
(Haut-Targeting, Dodge/Burn-Frequenztrennung, Retouch4me-orientiertes Profil).

## Inhalt

```
dodge_and_burn/
├── photocraft_dodge_burn.wasm        # aktuelles Plugin-Build (v1.7.0)
├── photocraft_dodge_burn_rust/       # Rust-Quelle, Tests, Doku
│   ├── src/lib.rs
│   ├── tests/test_plugin.rs
│   └── README.md                     # detaillierte Doku (Algorithmus, Parameter, Metriken)
└── README.md                         # ursprünglicher Prototyp-Text (Python-Pipeline)
```

## Plugin bauen & testen

```bash
cd dodge_and_burn/photocraft_dodge_burn_rust
./build.sh     # baut ../photocraft_dodge_burn.wasm (v1.7.0)
cargo test     # 9 Tests
```

## Plugin in Photocraft verwenden

1. `dodge_and_burn/photocraft_dodge_burn.wasm` in den Plugin-Ordner kopieren
   (Edit › Preferences › Plug-ins), Photocraft neu starten
2. Bild öffnen, Ebene duplizieren, **Filter › Plug-ins › Dodge & Burn Mask** auf der Kopie ausführen
3. Kopie über das Original legen, Mischmodus **Soft Light**, Deckkraft 60–85 %

## Aktueller Stand (v1.7.0)

- Alpha-Kanal der Ebene bleibt erhalten (keine halbtransparenten Masken mehr)
- adaptive Zellraster-Berechnung (Zeitbudget des Hosts wird eingehalten)
- kontinuierliche Basiskurve + röte-basierte Blemish-Spots + Burn-Cap −9 u8 / Burn-Floor −2 u8
- strukturelle Korrelation zur Retouch4me-Referenzmaske: 0,49 (Details in der Rust-README)

Details, Parameter, Metriken und Changelog: [dodge_and_burn/photocraft_dodge_burn_rust/README.md](dodge_and_burn/photocraft_dodge_burn_rust/README.md)

## Lizenz

 siehe [LICENSE](LICENSE)
