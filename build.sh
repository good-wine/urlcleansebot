#!/bin/bash
# Build script for URLCleanseBot with optimization options

set -e

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

# Default options
BUILD_TYPE="release"
STRIP=true
SHOW_TIME=true
VERBOSE=false

# Parse arguments
while [[ $# -gt 0 ]]; do
  case $1 in
    -d|--debug)
      BUILD_TYPE="debug"
      shift
      ;;
    --no-strip)
      STRIP=false
      shift
      ;;
    --no-time)
      SHOW_TIME=false
      shift
      ;;
    -v|--verbose)
      VERBOSE=true
      shift
      ;;
    -h|--help)
      echo "Usage: $0 [OPTIONS]"
      echo ""
      echo "Options:"
      echo "  -d, --debug       Build debug version (default: release)"
      echo "  --no-strip        Don't strip binary symbols"
      echo "  --no-time         Don't show build time"
      echo "  -v, --verbose     Verbose cargo output"
      echo "  -h, --help        Show this help message"
      exit 0
      ;;
    *)
      echo "Unknown option: $1"
      exit 1
      ;;
  esac
done

echo -e "${BLUE}🚀 URLCleanseBot Build Script${NC}"
echo ""

# Show build info
echo -e "${YELLOW}Build Configuration:${NC}"
echo "  Type: $BUILD_TYPE"
echo "  Strip: $STRIP"
echo "  Show time: $SHOW_TIME"
echo ""

# Build command
if [ "$BUILD_TYPE" = "debug" ]; then
  BUILD_CMD="cargo build"
  BINARY="target/debug/url_cleanse_bot"
else
  BUILD_CMD="cargo build --release"
  BINARY="target/release/url_cleanse_bot"
fi

# Add verbose flag
if [ "$VERBOSE" = true ]; then
  BUILD_CMD="$BUILD_CMD --verbose"
fi

# Build
echo -e "${YELLOW}Building...${NC}"
if [ "$SHOW_TIME" = true ]; then
  time $BUILD_CMD
else
  $BUILD_CMD
fi

# Check if binary exists
if [ ! -f "$BINARY" ]; then
  echo -e "${RED}❌ Build failed: Binary not found${NC}"
  exit 1
fi

# Get binary size before strip
SIZE_BEFORE=$(ls -lh "$BINARY" | awk '{print $5}')
echo ""
echo -e "${GREEN}✅ Build successful!${NC}"
echo "  Binary: $BINARY"
echo "  Size: $SIZE_BEFORE"

# Strip binary if requested
if [ "$STRIP" = true ] && [ "$BUILD_TYPE" = "release" ]; then
  echo ""
  echo -e "${YELLOW}Stripping binary...${NC}"
  strip "$BINARY"
  SIZE_AFTER=$(ls -lh "$BINARY" | awk '{print $5}')
  SAVED=$(($(stat -f%z "$BINARY" 2>/dev/null || stat -c%s "$BINARY" 2>/dev/null || echo 0)))
  echo -e "${GREEN}✅ Stripped successfully!${NC}"
  echo "  Size: $SIZE_BEFORE → $SIZE_AFTER"
fi

# Final summary
echo ""
echo -e "${BLUE}Summary:${NC}"
du -sh target/
echo ""
echo -e "${GREEN}Ready to deploy! 🎉${NC}"
