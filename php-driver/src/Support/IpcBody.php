<?php

declare(strict_types=1);

namespace Nusa\Octane\Support;

/**
 * Normalize request bodies from Rust IPC / embed JSON (string or byte array).
 */
final class IpcBody
{
    /**
     * @param mixed $body
     */
    #[\NoDiscard]
    public static function normalize(mixed $body): string
    {
        return match (true) {
            $body === null, $body === '' => '',
            is_string($body) => $body,
            is_array($body) => self::fromByteArray($body),
            default => (string) $body,
        };
    }

    /**
     * @param array<mixed> $body
     */
    private static function fromByteArray(array $body): string
    {
        $bytes = '';
        foreach ($body as $byte) {
            if (!is_int($byte) && !is_numeric($byte)) {
                continue;
            }
            $bytes .= chr((int) $byte);
        }

        return $bytes;
    }
}
