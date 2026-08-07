<?php

declare(strict_types=1);

namespace Nusa\Octane\Http;

/**
 * Normalize request URI/path for Laravel using PHP 8.5 URI extension when available.
 */
final class RequestPath
{
    /**
     * Path + query suitable for {@see \Illuminate\Http\Request::create()} first argument.
     */
    #[\NoDiscard]
    public static function forLaravel(string $uri): string
    {
        if ($uri === '') {
            return '/';
        }

        if ($uri[0] === '/') {
            return $uri;
        }

        if (extension_loaded('uri') && class_exists(\Uri\Rfc3986\Uri::class)) {
            try {
                $parsed = \Uri\Rfc3986\Uri::parse($uri);
                $path = $parsed->getPath();
                if ($path === '') {
                    $path = '/';
                }
                $query = $parsed->getQuery();

                return $query !== '' ? $path . '?' . $query : $path;
            } catch (\Throwable) {
                // fall through to parse_url
            }
        }

        $path = parse_url($uri, PHP_URL_PATH);
        $query = parse_url($uri, PHP_URL_QUERY);
        $path = is_string($path) && $path !== '' ? $path : '/';

        return is_string($query) && $query !== '' ? $path . '?' . $query : $path;
    }
}
