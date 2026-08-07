<?php

declare(strict_types=1);

namespace Nusa\Octane\Embed;

use Illuminate\Database\Connection;
use Illuminate\Foundation\Application;
use Nusa\Octane\Embed\Database\NusaAsyncSqliteConnection;

/**
 * P4-D embed async I/O gate + Laravel sqlite connection resolver.
 */
final class AsyncIo
{
    private static bool $resolverRegistered = false;

    public static function enabled(): bool
    {
        return getenv('NUSA_ASYNC_IO') === 'stub'
            && getenv('NUSA_EMBED_TRANSPORT') === 'frame';
    }

    /**
     * Read-only SQL allowlist (keep aligned with `nusa-core::async_io::is_readonly_sql`).
     */
    public static function isReadonlySql(string $sql): bool
    {
        $trimmed = trim($sql);
        if ($trimmed === '' || str_contains($trimmed, ';')) {
            return false;
        }
        $upper = strtoupper($trimmed);
        $first = preg_split('/\s+/', $upper, 2)[0] ?? '';

        return in_array($first, ['SELECT', 'WITH', 'PRAGMA', 'EXPLAIN'], true)
            && !str_contains($upper, ' INTO ')
            && !str_contains($upper, ' FOR UPDATE')
            && !($first === 'PRAGMA' && str_contains($upper, 'WRITABLE'));
    }

    /**
     * Wire Laravel sqlite + Rust stub env (call from {@see Runtime::bootstrap}).
     */
    public static function bootstrap(Application $app, string $appRoot): void
    {
        if (!self::enabled()) {
            return;
        }

        self::registerSqliteResolver();
        self::publishSqlitePath($app, $appRoot);
    }

    /**
     * Register before the HTTP kernel boots so the default connection uses the proxy.
     */
    public static function registerSqliteResolver(): void
    {
        if (self::$resolverRegistered) {
            return;
        }

        Connection::resolverFor(
            'sqlite',
            static fn (
                \PDO $connection,
                string $database,
                string $prefix,
                array $config,
            ): NusaAsyncSqliteConnection => new NusaAsyncSqliteConnection(
                $connection,
                $database,
                $prefix,
                $config,
            ),
        );

        self::$resolverRegistered = true;
    }

    public static function publishSqlitePath(Application $app, string $appRoot): void
    {
        $database = $app->make('config')->get('database.connections.sqlite.database');
        if (!is_string($database) || $database === '') {
            $database = $appRoot . '/database/database.sqlite';
        }
        if (is_file($database)) {
            putenv('NUSA_ASYNC_SQLITE_PATH=' . $database);
        }
    }
}
