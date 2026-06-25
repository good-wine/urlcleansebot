#!/usr/bin/env bash

# URLCleanseBot - Podman Deployment Script
# This script replaces Docker commands with Podman equivalents

set -e

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

# Function to print colored output
print_status() {
    echo -e "${GREEN}[INFO]${NC} $1"
}

print_warning() {
    echo -e "${YELLOW}[WARN]${NC} $1"
}

print_error() {
    echo -e "${RED}[ERROR]${NC} $1"
}

# Check if Podman is installed
if ! command -v podman &> /dev/null; then
    print_error "Podman is not installed. Please install Podman first."
    exit 1
fi

# Check if podman-compose is installed (optional)
if ! command -v podman-compose &> /dev/null; then
    print_warning "podman-compose is not installed. Using podman kube generate instead."
    COMPOSE_AVAILABLE=false
else
    COMPOSE_AVAILABLE=true
fi

# Function to build the container
build_container() {
    print_status "Building URLCleanseBot container..."
    podman build -t url_cleanse_bot -f Containerfile .
    print_status "Container built successfully!"
}

# Function to run the container
run_container() {
    print_status "Starting URLCleanseBot container..."
    
    # Create pod for networking
    podman pod create --name url_cleanse_bot_pod -p 3000:3000 2>/dev/null || true
    
    # Run the container
    podman run -d \
        --name url_cleanse_bot \
        --pod url_cleanse_bot_pod \
        --env-file .env \
        -e APP_ENV=production \
        -e RUST_LOG=url_cleanse_bot=info \
        -v ./bot.db:/app/bot.db:Z \
        --memory=512m \
        --cpus=0.5 \
        --restart=unless-stopped \
        url_cleanse_bot
    
    print_status "Container started successfully!"
}

# Function to stop the container
stop_container() {
    print_status "Stopping URLCleanseBot container..."
    podman stop url_cleanse_bot 2>/dev/null || true
    podman rm url_cleanse_bot 2>/dev/null || true
    podman pod rm url_cleanse_bot_pod 2>/dev/null || true
    print_status "Container stopped and removed!"
}

# Function to view logs
view_logs() {
    podman logs -f url_cleanse_bot
}

# Function to show status
show_status() {
    podman ps -a --filter name=url_cleanse_bot
}

# Main script logic
case "${1:-build}" in
    "build")
        build_container
        ;;
    "run")
        run_container
        ;;
    "start")
        build_container
        run_container
        ;;
    "stop")
        stop_container
        ;;
    "restart")
        stop_container
        run_container
        ;;
    "logs")
        view_logs
        ;;
    "status")
        show_status
        ;;
    "compose")
        if [ "$COMPOSE_AVAILABLE" = true ]; then
            print_status "Using podman-compose..."
            podman-compose -f podman-compose.yml "${2:-up}"
        else
            print_warning "podman-compose not available. Generate Kubernetes manifest..."
            podman kube generate url_cleanse_bot > url_cleanse_bot.yaml
            print_status "Kubernetes manifest generated: url_cleanse_bot.yaml"
            print_status "Use 'podman play kube url_cleanse_bot.yaml' to deploy"
        fi
        ;;
    "help"|"-h"|"--help")
        echo "URLCleanseBot - Podman Deployment Script"
        echo ""
        echo "Usage: $0 [command]"
        echo ""
        echo "Commands:"
        echo "  build     Build the container image"
        echo "  run       Run the container (assumes image exists)"
        echo "  start     Build and run the container"
        echo "  stop      Stop and remove the container"
        echo "  restart   Restart the container"
        echo "  logs      View container logs"
        echo "  status    Show container status"
        echo "  compose   Use podman-compose (optional argument: up/down)"
        echo "  help      Show this help message"
        echo ""
        echo "Examples:"
        echo "  $0 start           # Build and run the bot"
        echo "  $0 logs            # View logs"
        echo "  $0 restart         # Restart the bot"
        echo "  $0 compose up      # Use podman-compose to start"
        echo "  $0 compose down    # Use podman-compose to stop"
        ;;
    *)
        print_error "Unknown command: $1"
        print_status "Use '$0 help' to see available commands"
        exit 1
        ;;
esac