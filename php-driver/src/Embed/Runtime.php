<?php

declare(strict_types=1);

namespace Nusa\Octane\Embed;

use Illuminate\Contracts\Http\Kernel;
use Illuminate\Foundation\Application;
use Illuminate\Http\Request;
use Illuminate\Support\Facades\Facade;
use Nusa\Octane\Http\RequestPath;
use Nusa\Octane\Support\IpcBody;

/**
 * In-process / embed Laravel runtime (no IPC socket).
 *
 * Used by {@see nusa_embed_daemon.php} and future libphp FFI embed workers.
 */
final class Runtime
{
    private static ?Kernel $kernel = null;

    private static ?Application $app = null;

    /**
     * Boot Laravel once per PHP worker (Octane-style).
     */
    public static function bootstrap(string $appRoot): void
    {
        if (self::$kernel !== null) {
            return;
        }

        $previous = getcwd();
        chdir($appRoot);

        if (!is_file($appRoot . '/vendor/autoload.php')) {
            chdir($previous !== false ? $previous : $appRoot);
            throw new \RuntimeException(
                'Laravel app root not found (vendor/autoload.php missing at ' . $appRoot . ')'
            );
        }

        require $appRoot . '/vendor/autoload.php';

        /** @var Application $app */
        $app = require $appRoot . '/bootstrap/app.php';

        if (AsyncIo::enabled()) {
            AsyncIo::registerSqliteResolver();
        }

        $kernel = $app->make(Kernel::class);
        $kernel->bootstrap();

        Facade::clearResolvedInstances();
        Facade::setFacadeApplication($app);

        self::$app = $app;
        self::$kernel = $kernel;

        if (AsyncIo::enabled()) {
            AsyncIo::publishSqlitePath($app, $appRoot);
        }

        if ($previous !== false) {
            chdir($previous);
        }
    }

    /**
     * Handle one HTTP request (array contract matches IPC Worker).
     *
     * @param array<string, mixed> $request
     * @return array<string, mixed>
     */
    public static function handleRequest(array $request): array
    {
        if (self::$kernel === null) {
            return [
                'type' => 'Response',
                'status' => 503,
                'headers' => ['Content-Type' => ['text/plain']],
                'body' => 'embed runtime not bootstrapped',
            ];
        }

        $method = $request['method'] ?? 'GET';
        $uri = $request['uri'] ?? '/';
        $headers = $request['headers'] ?? [];
        $body = self::normalizeBody($request['body'] ?? '');
        $cookies = $request['cookies'] ?? [];
        if (!is_array($cookies)) {
            $cookies = [];
        }

        $_SERVER['REQUEST_METHOD'] = $method;
        $_SERVER['REQUEST_URI'] = $uri;
        $_SERVER['HTTP_HOST'] = $headers['Host'][0] ?? $headers['host'][0] ?? 'localhost';

        foreach ($headers as $name => $values) {
            $key = 'HTTP_' . strtoupper(str_replace('-', '_', (string) $name));
            $_SERVER[$key] = is_array($values) ? implode(', ', $values) : (string) $values;
        }

        if ($body !== '') {
            $_SERVER['CONTENT_LENGTH'] = (string) strlen($body);
            if (!isset($headers['Content-Type']) && !isset($headers['content-type'])) {
                $_SERVER['CONTENT_TYPE'] = 'application/x-www-form-urlencoded';
                $_SERVER['HTTP_CONTENT_TYPE'] = 'application/x-www-form-urlencoded';
            }
        }

        $laravelRequest = Request::create($uri, $method, [], $cookies, [], $_SERVER, $body);
        $symfonyResponse = self::$kernel->handle($laravelRequest);

        $responseHeaders = [];
        foreach ($symfonyResponse->headers->all() as $name => $values) {
            $responseHeaders[$name] = $values;
        }

        self::$kernel->terminate($laravelRequest, $symfonyResponse);

        return [
            'type' => 'Response',
            'id' => $request['id'] ?? null,
            'status' => $symfonyResponse->getStatusCode(),
            'headers' => $responseHeaders,
            'body' => (string) $symfonyResponse->getContent(),
            'terminated' => true,
        ];
    }

    /**
     * Reset Octane-style state between requests (listeners may hook here).
     */
    public static function resetRequestState(): void
    {
        if (self::$app !== null) {
            Facade::clearResolvedInstance('request');
        }
    }

}
