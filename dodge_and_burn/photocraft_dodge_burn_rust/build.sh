#!/bin/bash
# Build-Skript für Photocraft Dodge & Burn Plugin (Rust/WASM)

set -e

echo "=========================================="
echo "Photocraft Dodge & Burn Plugin - Build"
echo "=========================================="

# 1. Rust Toolchain für WASM prüfen
if ! rustup target list | grep -q wasm32-unknown-unknown; then
    echo "[!] WASM-Target fehlt. Installiere es..."
    rustup target add wasm32-unknown-unknown
fi

# 2. Plugin kompilieren
echo "[*] Kompiliere Plugin für WASM..."
cargo build --release --target wasm32-unknown-unknown

# 3. Dateigröße anzeigen
WASM_FILE="target/wasm32-unknown-unknown/release/photocraft_dodge_burn.wasm"
if [ -f "$WASM_FILE" ]; then
    SIZE=$(stat -f%z "$WASM_FILE" 2>/dev/null || stat -c%s "$WASM_FILE")
    echo "[+] Plugin kompiliert: $WASM_FILE ($SIZE Bytes)"
else
    echo "[!] Fehler: WASM-Datei nicht gefunden!"
    exit 1
fi

# 4. Plugin in Projektordner kopieren
echo "[*] Kopiere Plugin nach photocraft_dodge_burn/..."
cp "$WASM_FILE" ../photocraft_dodge_burn/photocraft_dodge_burn.wasm

echo "[✔] Plugin erfolgreich erstellt!"
echo ""
echo "Um das Plugin in Photocraft zu installieren:"
echo "1. Kopiere photocraft_dodge_burn.wasm in deinen Photocraft-Plugins-Ordner"
echo "2. Starte Photocraft neu"
echo "3. Plugin erscheint unter Filter > Plug-ins > Dodge & Burn Mask"
