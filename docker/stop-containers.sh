#!/bin/sh
set -e

echo "Stopping TinyIoTHub containers..."

for container in tinyiothub-nginx tinyiothub-web tinyiothub-api; do
    if docker ps --format '{{.Names}}' | grep -q "^${container}$"; then
        echo "Stopping ${container}..."
        docker stop ${container}
    fi
done

echo "All containers stopped."
