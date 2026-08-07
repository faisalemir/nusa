<?php

declare(strict_types=1);

namespace Nusa\Octane\Support;

/**
 * Persistent cURL share handle across Octane requests (PHP 8.5+).
 *
 * Reduces connection setup when the app issues repeated outbound HTTP calls.
 */
final class OctaneCurlShare
{
    private static ?\CurlShareHandle $share = null;

    public static function instance(): ?\CurlShareHandle
    {
        if (!function_exists('curl_share_init_persistent')) {
            return null;
        }

        if (self::$share === null) {
            $share = curl_share_init_persistent();
            if ($share === false) {
                return null;
            }
            curl_share_setopt($share, CURLSHOPT_SHARE, CURL_LOCK_DATA_DNS);
            curl_share_setopt($share, CURLSHOPT_SHARE, CURL_LOCK_DATA_SSL_SESSION);
            self::$share = $share;
        }

        return self::$share;
    }

    public static function reset(): void
    {
        self::$share = null;
    }
}
