#!/bin/bash
#
# URLCleanseBot - Setup & Validation Script
# 
# This script helps you configure the bot by:
# - Checking required dependencies
# - Validating environment variables
# - Testing API keys
# - Providing setup guidance
#

set -e

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

# Helper functions
print_header() {
    echo -e "\n${BLUE}━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━${NC}"
    echo -e "${BLUE}$1${NC}"
    echo -e "${BLUE}━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━${NC}\n"
}

print_success() {
    echo -e "${GREEN}✓${NC} $1"
}

print_error() {
    echo -e "${RED}✗${NC} $1"
}

print_warning() {
    echo -e "${YELLOW}⚠${NC} $1"
}

print_info() {
    echo -e "${BLUE}ℹ${NC} $1"
}

# Check if .env exists
check_env_file() {
    print_header "Checking Environment Configuration"
    
    if [ ! -f .env ]; then
        print_error ".env file not found"
        print_info "Creating .env from .env.example..."
        
        if [ -f .env.example ]; then
            cp .env.example .env
            print_success ".env file created"
            print_warning "Please edit .env with your configuration before continuing"
            exit 0
        else
            print_error ".env.example not found"
            exit 1
        fi
    else
        print_success ".env file found"
    fi
}

# Load environment variables
load_env() {
    if [ -f .env ]; then
        export $(cat .env | grep -v '^#' | grep -v '^[[:space:]]*$' | xargs)
    fi
}

# Check Rust installation
check_rust() {
    print_header "Checking Dependencies"
    
    if command -v rustc &> /dev/null; then
        RUST_VERSION=$(rustc --version | awk '{print $2}')
        print_success "Rust installed: $RUST_VERSION"
        
        RUST_MAJOR=$(echo $RUST_VERSION | cut -d. -f1)
        RUST_MINOR=$(echo $RUST_VERSION | cut -d. -f2)
        
        if [ "$RUST_MAJOR" -ge 1 ] && [ "$RUST_MINOR" -ge 88 ]; then
            print_success "Rust version is compatible (>= 1.88)"
        else
            print_warning "Rust version < 1.88 detected. Update recommended: rustup update"
        fi
    else
        print_error "Rust not installed"
        print_info "Install Rust: curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh"
        exit 1
    fi
    
    if command -v cargo &> /dev/null; then
        print_success "Cargo installed"
    else
        print_error "Cargo not found"
        exit 1
    fi
}

# Validate required environment variables
validate_required_vars() {
    print_header "Validating Required Configuration"
    
    local all_valid=true
    
    # TELOXIDE_TOKEN
    if [ -z "$TELOXIDE_TOKEN" ] || [ "$TELOXIDE_TOKEN" = "your_bot_token" ]; then
        print_error "TELOXIDE_TOKEN not configured"
        print_info "Get your bot token from @BotFather on Telegram"
        all_valid=false
    else
        print_success "TELOXIDE_TOKEN configured"
    fi
    
    # BOT_USERNAME
    if [ -z "$BOT_USERNAME" ] || [ "$BOT_USERNAME" = "@your_bot_username" ]; then
        print_error "BOT_USERNAME not configured"
        all_valid=false
    else
        print_success "BOT_USERNAME configured: $BOT_USERNAME"
    fi
    
    # ADMIN_ID
    if [ -z "$ADMIN_ID" ] || [ "$ADMIN_ID" = "your_telegram_user_id" ]; then
        print_error "ADMIN_ID not configured"
        print_info "Send /start to the bot to get your User ID"
        all_valid=false
    else
        print_success "ADMIN_ID configured: $ADMIN_ID"
    fi
    
    # DATABASE_URL
    if [ -z "$DATABASE_URL" ]; then
        print_warning "DATABASE_URL not set, will use default (sqlite:bot.db)"
    else
        print_success "DATABASE_URL configured"
    fi
    
    if [ "$all_valid" = false ]; then
        echo ""
        print_error "Required configuration missing. Please edit .env file."
        exit 1
    fi
}

# Validate optional integrations
validate_optional_vars() {
    print_header "Checking Optional Integrations"
    
    # VirusTotal
    if [ -n "$VIRUSTOTAL_API_KEY" ] && [ "$VIRUSTOTAL_API_KEY" != "your_virustotal_api_key_here" ]; then
        print_success "VirusTotal API key configured"
        
        # Check alert mode
        if [ "${VIRUSTOTAL_ALERT_ONLY:-true}" = "true" ]; then
            print_info "  └─ Alert-only mode: ENABLED (default)"
        else
            print_info "  └─ Alert-only mode: DISABLED (sends all results)"
        fi
        
        # Test API key validity (optional)
        print_info "  └─ Testing API key..."
        TEST_URL="https://www.google.com"
        ENCODED_URL=$(echo -n "$TEST_URL" | base64 -w 0 | tr '+/' '-_' | tr -d '=')
        HTTP_CODE=$(curl -s -o /dev/null -w "%{http_code}" -H "x-apikey: $VIRUSTOTAL_API_KEY" \
            "https://www.virustotal.com/api/v3/urls/$ENCODED_URL" || echo "000")
        
        if [ "$HTTP_CODE" = "200" ] || [ "$HTTP_CODE" = "404" ]; then
            print_success "  └─ API key valid"
        elif [ "$HTTP_CODE" = "401" ]; then
            print_error "  └─ API key invalid or expired"
        elif [ "$HTTP_CODE" = "429" ]; then
            print_warning "  └─ Rate limit reached, cannot verify"
        else
            print_warning "  └─ Could not verify API key (HTTP $HTTP_CODE)"
        fi
    else
        print_info "VirusTotal: Not configured (optional)"
    fi
    
    # URLScan.io
    if [ -n "$URLSCAN_API_KEY" ] && [ "$URLSCAN_API_KEY" != "your_urlscan_api_key_here" ]; then
        print_success "URLScan.io API key configured"
        
        # Check alert mode
        if [ "${URLSCAN_ALERT_ONLY:-true}" = "true" ]; then
            print_info "  └─ Alert-only mode: ENABLED (default)"
        else
            print_info "  └─ Alert-only mode: DISABLED (sends all results)"
        fi
        
        # Note: URLScan.io testing would require submitting a scan, skipping
        print_info "  └─ API key will be validated on first use"
    else
        print_info "URLScan.io: Not configured (optional)"
    fi
    
    # AI Deep Scan
    if [ -n "$AI_API_KEY" ] && [ "$AI_API_KEY" != "your_ai_api_key" ]; then
        print_success "AI Deep Scan configured"
        print_info "  └─ Model: ${AI_MODEL:-gpt-4}"
        print_info "  └─ Endpoint: ${AI_API_BASE:-https://api.openai.com/v1}"
    else
        print_info "AI Deep Scan: Not configured (optional)"
    fi
}

# Check database
check_database() {
    print_header "Database Configuration"
    
    if [[ "$DATABASE_URL" == sqlite:* ]]; then
        DB_FILE=$(echo $DATABASE_URL | sed 's/sqlite:\([^?]*\).*/\1/')
        print_info "Using SQLite database: $DB_FILE"
        
        if [ -f "$DB_FILE" ]; then
            DB_SIZE=$(du -h "$DB_FILE" | cut -f1)
            print_success "Database exists (${DB_SIZE})"
        else
            print_info "Database will be created on first run"
        fi
    elif [[ "$DATABASE_URL" == postgres:* ]]; then
        print_info "Using PostgreSQL database"
        print_success "Production-ready configuration"
    else
        print_warning "Unknown database type"
    fi
}

# Check port availability
check_ports() {
    print_header "Checking Port Availability"
    
    SERVER_PORT=$(echo ${SERVER_ADDR:-0.0.0.0:4000} | cut -d: -f2)
    
    if command -v lsof &> /dev/null; then
        if lsof -Pi :$SERVER_PORT -sTCP:LISTEN -t >/dev/null 2>&1; then
            print_warning "Port $SERVER_PORT already in use"
            print_info "You may need to change SERVER_ADDR in .env"
        else
            print_success "Port $SERVER_PORT available"
        fi
    else
        print_info "lsof not available, skipping port check"
    fi
}

# Summary and next steps
print_summary() {
    print_header "Setup Summary"
    
    echo -e "${GREEN}✓ Configuration validated successfully!${NC}\n"
    
    print_info "Next steps:"
    echo "  1. Build the project:    ${YELLOW}cargo build --release${NC}"
    echo "  2. Run the bot:          ${YELLOW}cargo run --release${NC}"
    echo "  3. Or use Podman:        ${YELLOW}./podman-deploy.sh start${NC}"
    echo ""
    print_info "Useful commands:"
    echo "  - Check logs:            ${YELLOW}tail -f bot.log${NC}"
    echo "  - View settings:         ${YELLOW}Send /settings to the bot${NC}"
    echo "  - Check status:          ${YELLOW}./podman-deploy.sh status${NC}"
    echo ""
    print_info "Documentation:"
    echo "  - Architecture:          ${YELLOW}docs/ARCHITECTURE.md${NC}"
    echo "  - Deployment:            ${YELLOW}docs/DEPLOYMENT.md${NC}"
    echo ""
}

# Main execution
main() {
    echo -e "\n${BLUE}╔═══════════════════════════════════════════════╗${NC}"
    echo -e "${BLUE}║                                               ║${NC}"
    echo -e "${BLUE}║       URLCleanseBot - Setup Validator         ║${NC}"
    echo -e "${BLUE}║                                               ║${NC}"
    echo -e "${BLUE}╚═══════════════════════════════════════════════╝${NC}"
    
    check_rust
    check_env_file
    load_env
    validate_required_vars
    validate_optional_vars
    check_database
    check_ports
    print_summary
    
    echo -e "${GREEN}Setup validation complete! 🚀${NC}\n"
}

# Run if executed directly
if [ "${BASH_SOURCE[0]}" = "${0}" ]; then
    main "$@"
fi
