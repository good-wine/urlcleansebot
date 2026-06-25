#!/bin/bash
# Automated database backup script with retention policy
# Usage: ./backup_db.sh [database_url] [backup_dir]

set -euo pipefail

# Configuration
DATABASE_URL="${1:-${DATABASE_URL:-sqlite:urlcleanse.db}}"
BACKUP_DIR="${2:-./backups}"
RETENTION_DAYS="${BACKUP_RETENTION_DAYS:-30}"
MAX_BACKUPS="${MAX_BACKUPS:-10}"
TIMESTAMP=$(date +%Y%m%d_%H%M%S)

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m'

log_info() {
    echo -e "${GREEN}[INFO]${NC} $1"
}

log_warn() {
    echo -e "${YELLOW}[WARN]${NC} $1"
}

log_error() {
    echo -e "${RED}[ERROR]${NC} $1"
}

# Create backup directory
mkdir -p "$BACKUP_DIR"
log_info "Backup directory: $BACKUP_DIR"

# SQLite backup
if [[ "$DATABASE_URL" == sqlite:* ]]; then
    DB_PATH="${DATABASE_URL#sqlite:}"
    
    if [[ ! -f "$DB_PATH" ]]; then
        log_error "Database file not found: $DB_PATH"
        exit 1
    fi
    
    BACKUP_FILE="$BACKUP_DIR/urlcleanse_backup_$TIMESTAMP.db"
    log_info "Creating SQLite backup..."
    
    if command -v sqlite3 &> /dev/null; then
        sqlite3 "$DB_PATH" ".backup '$BACKUP_FILE'"
    else
        cp "$DB_PATH" "$BACKUP_FILE"
    fi
    
    log_info "Compressing backup..."
    gzip "$BACKUP_FILE"
    BACKUP_FILE="$BACKUP_FILE.gz"
fi

# Verify and cleanup
if [[ -f "$BACKUP_FILE" ]]; then
    SIZE=$(du -h "$BACKUP_FILE" | cut -f1)
    log_info "Backup created: $BACKUP_FILE ($SIZE)"
    
    # Cleanup old backups
    find "$BACKUP_DIR" -name "urlcleanse_backup_*.db.gz" -type f -mtime +$RETENTION_DAYS -delete 2>/dev/null || true
    
    REMAINING=$(find "$BACKUP_DIR" -name "urlcleanse_backup_*" -type f | wc -l)
    log_info "Total backups: $REMAINING"
else
    log_error "Backup failed"
    exit 1
fi
