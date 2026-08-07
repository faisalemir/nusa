<?php

declare(strict_types=1);

/**
 * Load `nusa/octane` classes when Composer vendor symlinks are unavailable (e.g. Windows dev).
 */
function nusa_embed_ensure_driver_loaded(string $appRoot): void
{
    if (class_exists(\Nusa\Octane\Embed\Runtime::class, false)) {
        return;
    }

    $candidates = array_values(array_filter([
        getenv('NUSA_PHP_DRIVER') ?: '',
        dirname(__DIR__),
        $appRoot . '/php-driver',
    ], static fn (string $path): bool => $path !== '' && is_dir($path)));

    foreach ($candidates as $root) {
        $src = $root . '/src';
        if (!is_file($src . '/Embed/Runtime.php')) {
            continue;
        }

        spl_autoload_register(
            static function (string $class) use ($src): void {
                if (!str_starts_with($class, 'Nusa\\Octane\\')) {
                    return;
                }
                $relative = substr($class, strlen('Nusa\\Octane\\'));
                $file = $src . '/' . str_replace('\\', '/', $relative) . '.php';
                if (is_file($file)) {
                    require $file;
                }
            },
            prepend: true,
        );

        return;
    }

    throw new RuntimeException(
        'nusa/octane driver not found (composer install nusa/octane or set NUSA_PHP_DRIVER)'
    );
}

/**
 * C/FFI embed entrypoints for libphp ZTS (future in-process workers).
 *
 * @package Nusa\Octane
 */

/**
 * Bootstrap Laravel in the current embed thread (call once per worker).
 */
function nusa_embed_bootstrap(): void
{
    $root = getenv('NUSA_CODE_DIR') ?: getcwd();
    if ($root === false || $root === '') {
        throw new RuntimeException('NUSA_CODE_DIR or cwd required for embed bootstrap');
    }
    $autoload = $root . '/vendor/autoload.php';
    if (!is_file($autoload)) {
        throw new RuntimeException('vendor/autoload.php missing at ' . $root);
    }
    require_once $autoload;
    nusa_embed_ensure_driver_loaded($root);
    \Nusa\Octane\Embed\Runtime::bootstrap($root);
}

/**
 * Handle one HTTP request from the Rust embed worker.
 *
 * @param array<string, mixed> $request
 * @return array<string, mixed>
 */
function nusa_embed_handle_request(array $request): array
{
    $response = \Nusa\Octane\Embed\Runtime::handleRequest($request);
    \Nusa\Octane\Embed\Runtime::resetRequestState();

    return $response;
}

/**
 * Optional explicit reset hook for Rust state-reset orchestrator.
 */
function nusa_embed_reset(): void
{
    \Nusa\Octane\Embed\Runtime::resetRequestState();
}
