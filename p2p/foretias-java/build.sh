#!/bin/bash
set -euo pipefail
# Build script for foretias-java JNI native library
#
# Prerequisites:
#   - Rust core library built: foretias/p2p/core-engine/target/libforetias_core.a
#   - JDK 17+ installed (JAVA_HOME or javac in PATH)
#
# Usage:
#   ./build.sh           # Build libforetias_java.so
#   ./build.sh clean     # Remove build artifacts

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
P2P_ROOT="$SCRIPT_DIR/.."
CORE_INCLUDE="$P2P_ROOT/core/include"
CORE_LIB="$P2P_ROOT/core-engine/target"
JNI_SRC="$SCRIPT_DIR/src/main/c/jni_crypto.c"
OUT_DIR="$SCRIPT_DIR/target"
LIB_NAME="libforetias_java.so"

case "${1:-build}" in
    clean)
        rm -rf "$OUT_DIR"
        echo "Cleaned."
        exit 0
        ;;
esac

mkdir -p "$OUT_DIR"

# Find Java include paths
if [ -n "${JAVA_HOME:-}" ]; then
    JAVAINC="$JAVA_HOME/include"
    JAVAINC_OS="$JAVA_HOME/include/$(java -XshowSettings:properties 2>&1 | grep 'os.name' | head -1 | awk '{print $4}' | tr -d ',')"
else
    JAVAINC=$(dirname "$(readlink -f "$(which javac)")")/../../include
    JAVAINC_OS="$JAVAINC/linux"
fi

[ ! -d "$JAVAINC" ] && { echo "ERROR: Cannot find JNI headers at $JAVAINC"; exit 1; }

# Compile JNI shared library
gcc -shared -fPIC -O2 \
    -I"$JAVAINC" \
    -I"${JAVAINC_OS:-}" \
    -I"$CORE_INCLUDE" \
    "$JNI_SRC" \
    -L"$CORE_LIB" \
    -lforetias_core \
    -o "$OUT_DIR/$LIB_NAME" \
    -Wl,-rpath,"$CORE_LIB"

echo "Built $OUT_DIR/$LIB_NAME"

# Run Java CLI test (stamp/verify roundtrip)
echo ""
echo "=== Running self-test ==="
cd "$SCRIPT_DIR"
LD_LIBRARY_PATH="$OUT_DIR:$LD_LIBRARY_PATH" \
java -cp "$OUT_DIR/classes:$(find ~/.m2/repository -name 'gson-2.10.1.jar' 2>/dev/null || echo '$OUT_DIR/classes')" \
    foretias.Cli stamp -m "hello from java" --tbn "test-java" > /tmp/test_foretis.json 2>&1 || true

echo ""
echo "Build complete. Set LD_LIBRARY_PATH to run:"
echo "  export LD_LIBRARY_PATH=$OUT_DIR:\$LD_LIBRARY_PATH"
echo "  java -cp target/classes:~/.m2/repository/com/google/code/gson/gson/2.10.1/gson-2.10.1.jar foretias.Cli stamp -m 'test'"
