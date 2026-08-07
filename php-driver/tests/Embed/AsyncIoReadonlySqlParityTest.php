<?php

declare(strict_types=1);

namespace Nusa\Octane\Tests\Embed;

use NUnit\Framework\Attributes\DataProvider;
use Nusa\Octane\Embed\AsyncIo;

/**
 * S20: PHP AsyncIo.php — isReadonlySql parity with Rust nusa-core::async_io::is_readonly_sql.
 *
 * These are contract tests: both sides must produce identical results for every SQL pattern.
 * Run with: `php vendor/bin/phpunit tests/Embed/AsyncIoReadonlySqlParityTest.php`
 * Or as a standalone: `php tests/Embed/AsyncIoReadonlySqlParityTest.php`
 *
 * STUB_CONTRACT: PHP tests require PHP 8.3+ with the php-driver autoloader.
 * On Alpine CI: run inside `nusa-test-runner` with `just podman-test-laravel`.
 */
class AsyncIoReadonlySqlParityTest
{
    /**
     * Cases where BOTH PHP and Rust must return true (read-only allowed).
     */
    private static array $readonlyCases = [
        'simple select'             => 'SELECT 1',
        'select lowercase'          => 'select 1',
        'select with trim'          => '  SELECT 1  ',
        'select with newline'       => "\nSELECT 1\n",
        'select with tab'           => "\tSELECT\t1",
        'select count'              => 'SELECT count(*) FROM users',
        'select with WHERE'         => 'SELECT * FROM users WHERE id = 1',
        'deeply nested select'      => 'SELECT (SELECT (SELECT 1))',
        'WITH CTE select'           => 'WITH t AS (SELECT 1) SELECT * FROM t',
        'PRAGMA journal_mode'       => 'PRAGMA journal_mode=WAL',
        'PRAGMA table_info'         => 'PRAGMA table_info(users)',
        'PRAGMA without value'      => 'PRAGMA journal_mode',
        'EXPLAIN query'             => 'EXPLAIN SELECT * FROM users',
        'EXPLAIN with uppercase'    => 'explain select 1',
        'comment with space'        => 'SELECT /* safe */ 1',
        'multiline select'          => "SELECT\n1",
    ];

    /**
     * Cases where BOTH PHP and Rust must return false (write / rejected).
     */
    private static array $writeCases = [
        'INSERT'                    => 'INSERT INTO x VALUES (1)',
        'UPDATE'                    => 'UPDATE x SET y = 1',
        'DELETE'                    => 'DELETE FROM x',
        'DROP'                      => 'DROP TABLE x',
        'CREATE'                    => 'CREATE TABLE x (id INT)',
        'ALTER'                     => 'ALTER TABLE x ADD COLUMN y INT',
        'stacked queries'           => 'SELECT 1; DROP TABLE x',
        'single statement semicolon'=> 'SELECT 1;',
        'SELECT INTO'               => "SELECT * INTO OUTFILE '/tmp/x'",
        'SELECT FOR UPDATE'         => 'SELECT * FROM x FOR UPDATE',
        'PRAGMA writable_schema'    => 'PRAGMA writable_schema=1',
        'ATTACH database'           => "ATTACH DATABASE 'evil.db' AS evil",
        'DETACH database'           => 'DETACH DATABASE evil',
        'BEGIN TRANSACTION'         => 'BEGIN TRANSACTION',
        'COMMIT'                    => 'COMMIT',
        'ROLLBACK'                  => 'ROLLBACK',
        'VACUUM'                    => 'VACUUM',
        'REINDEX'                   => 'REINDEX',
        'empty string'              => '',
        'whitespace only'           => '   ',
        'multiline injection'       => "SELECT\n1; DROP TABLE x",
        'CTE write body'            => 'WITH x AS (SELECT 1) INSERT INTO y SELECT * FROM x',
    ];

    public static function readonlySqlCases(): array
    {
        $cases = [];
        foreach (self::$readonlyCases as $name => $sql) {
            $cases[$name] = [$sql, true];
        }
        foreach (self::$writeCases as $name => $sql) {
            $cases[$name] = [$sql, false];
        }
        return $cases;
    }

    #[DataProvider('readonlySqlCases')]
    public function testIsReadonlySqlParity(string $sql, bool $expected): void
    {
        $result = AsyncIo::isReadonlySql($sql);
        self::assertSame(
            $expected,
            $result,
            sprintf(
                "isReadonlySql(%s) = %s, expected %s",
                json_encode($sql),
                $result ? 'true' : 'false',
                $expected ? 'true' : 'false'
            )
        );
    }

    public function testEnabledRequiresBothEnvVars(): void
    {
        // Neither set
        putenv('NUSA_ASYNC_IO');
        putenv('NUSA_EMBED_TRANSPORT');
        self::assertFalse(AsyncIo::enabled());

        // Only async IO set
        putenv('NUSA_ASYNC_IO=stub');
        putenv('NUSA_EMBED_TRANSPORT');
        self::assertFalse(AsyncIo::enabled());

        // Only transport set
        putenv('NUSA_ASYNC_IO');
        putenv('NUSA_EMBED_TRANSPORT=frame');
        self::assertFalse(AsyncIo::enabled());

        // Both set
        putenv('NUSA_ASYNC_IO=stub');
        putenv('NUSA_EMBED_TRANSPORT=frame');
        self::assertTrue(AsyncIo::enabled());

        // Cleanup
        putenv('NUSA_ASYNC_IO');
        putenv('NUSA_EMBED_TRANSPORT');
    }

    public function testEnabledMaliciousValues(): void
    {
        $maliciousAsync = ['evil', 'DROP TABLE', '../../etc/passwd', '', '1', 'true'];
        foreach ($maliciousAsync as $value) {
            putenv("NUSA_ASYNC_IO={$value}");
            putenv('NUSA_EMBED_TRANSPORT=frame');
            self::assertFalse(
                AsyncIo::enabled(),
                "NUSA_ASYNC_IO={$value} + frame must NOT enable async I/O"
            );
        }

        $maliciousTransport = ['evil', 'binary', 'json', '', 'frame_v2'];
        foreach ($maliciousTransport as $value) {
            putenv('NUSA_ASYNC_IO=stub');
            putenv("NUSA_EMBED_TRANSPORT={$value}");
            self::assertFalse(
                AsyncIo::enabled(),
                "stub + NUSA_EMBED_TRANSPORT={$value} must NOT enable async I/O"
            );
        }

        putenv('NUSA_ASYNC_IO');
        putenv('NUSA_EMBED_TRANSPORT');
    }

    public function testRegisterSqliteResolverIdempotent(): void
    {
        // Calling twice should not throw
        AsyncIo::registerSqliteResolver();
        AsyncIo::registerSqliteResolver();
        // If we reach here, idempotency works
        self::assertTrue(true);
    }
}

// Standalone runner fallback (when PHPUnit not available)
if (!class_exists('PHPUnit\Framework\TestCase') && php_sapi_name() === 'cli') {
    echo "=== AsyncIo isReadonlySql Parity Tests (Standalone) ===\n\n";

    $pass = 0;
    $fail = 0;

    foreach (AsyncIoReadonlySqlParityTest::$readonlyCases as $name => $sql) {
        $result = AsyncIo::isReadonlySql($sql);
        if ($result === true) {
            echo "  PASS: {$name}\n";
            $pass++;
        } else {
            echo "  FAIL: {$name} — got false, expected true\n";
            $fail++;
        }
    }

    foreach (AsyncIoReadonlySqlParityTest::$writeCases as $name => $sql) {
        $result = AsyncIo::isReadonlySql($sql);
        if ($result === false) {
            echo "  PASS: {$name}\n";
            $pass++;
        } else {
            echo "  FAIL: {$name} — got true, expected false\n";
            $fail++;
        }
    }

    echo "\n=== Enabled() Tests ===\n\n";

    // Test 1: neither set
    putenv('NUSA_ASYNC_IO'); putenv('NUSA_EMBED_TRANSPORT');
    if (!AsyncIo::enabled()) { echo "  PASS: neither set → disabled\n"; $pass++; }
    else { echo "  FAIL: neither set → should be disabled\n"; $fail++; }

    // Test 2: both set
    putenv('NUSA_ASYNC_IO=stub'); putenv('NUSA_EMBED_TRANSPORT=frame');
    if (AsyncIo::enabled()) { echo "  PASS: both set → enabled\n"; $pass++; }
    else { echo "  FAIL: both set → should be enabled\n"; $fail++; }

    // Test 3: only async IO
    putenv('NUSA_ASYNC_IO=stub'); putenv('NUSA_EMBED_TRANSPORT');
    if (!AsyncIo::enabled()) { echo "  PASS: only async → disabled\n"; $pass++; }
    else { echo "  FAIL: only async → should be disabled\n"; $fail++; }

    putenv('NUSA_ASYNC_IO'); putenv('NUSA_EMBED_TRANSPORT');

    echo "\n=== Results ===\n";
    echo "  Passed: {$pass}\n";
    echo "  Failed: {$fail}\n";
    echo "  Total:  " . ($pass + $fail) . "\n";

    exit($fail > 0 ? 1 : 0);
}
