<?php
/**
 * Nusa Octane Configuration.
 *
 * @package Nusa\Octane
 */

return [
    /*
    |--------------------------------------------------------------------------
    | Nusa Octane Worker Configuration
    |--------------------------------------------------------------------------
    |
    | These options configure the Nusa Rust orchestrator's behavior when
    | managing Laravel Octane worker processes.
    |
    */

    'workers' => env('NUSA_MAX_WORKERS', 4),

    /*
    |--------------------------------------------------------------------------
    | Memory Limit
    |--------------------------------------------------------------------------
    |
    | Maximum RSS memory (in MB) before a worker is recycled.
    | Set to 0 to disable memory-based recycling.
    |
    */
    'max_memory_mb' => env('NUSA_MAX_MEMORY_MB', 512),

    /*
    |--------------------------------------------------------------------------
    | Request Limit
    |--------------------------------------------------------------------------
    |
    | Maximum number of requests before a worker is recycled.
    | Set to 0 to disable request-count-based recycling.
    |
    */
    'max_requests' => env('NUSA_MAX_REQUESTS', 1000),

    /*
    |--------------------------------------------------------------------------
    | Timeout
    |--------------------------------------------------------------------------
    |
    | Maximum request execution time in milliseconds.
    |
    */
    'timeout_ms' => env('NUSA_TIMEOUT_MS', 30000),

    /*
    |--------------------------------------------------------------------------
    | IPC Socket Directory
    |--------------------------------------------------------------------------
    |
    | Directory where Unix domain sockets are created for worker IPC.
    |
    */
    'socket_dir' => env('NUSA_SOCKET_DIR', storage_path('nusa')),

    /*
    |--------------------------------------------------------------------------
    | State Reset
    |--------------------------------------------------------------------------
    |
    | Whether to reset application state between requests.
    | Always true for Nusa — the orchestrator handles this automatically.
    |
    */
    'reset_state' => true,

    /*
    |--------------------------------------------------------------------------
    | Telemetry
    |--------------------------------------------------------------------------
    |
    | Whether to report worker metrics (RSS, request count, latency)
    | to the Nusa telemetry system.
    |
    */
    'telemetry' => env('NUSA_TELEMETRY', true),
];
