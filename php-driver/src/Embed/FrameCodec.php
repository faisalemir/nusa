<?php

declare(strict_types=1);

namespace Nusa\Octane\Embed;

/**
 * Binary embed transport v1 (`NEB1`) — P4-B spike.
 *
 * Same outer envelope as JSON stdio: 4-byte LE length + payload.
 *
 * @internal
 */
final class FrameCodec
{
    private const string MAGIC = 'NEB1';

    private const int VERSION = 1;

    private const int OP_BOOTSTRAP = 1;

    private const int OP_ACK = 2;

    private const int OP_REQUEST = 3;

    private const int OP_RESPONSE = 4;

    private const int OP_ERROR = 5;

    private const int OP_ASYNC_QUERY = 6;

    private const int OP_ASYNC_RESULT = 7;

    /**
     * @return array<string, mixed>|null
     */
    public static function readMessage(): ?array
    {
        $payload = self::readLengthPrefixed();
        if ($payload === null) {
            return null;
        }
        if (strlen($payload) < 8 || substr($payload, 0, 4) !== self::MAGIC) {
            return null;
        }
        $op = ord($payload[5]);
        $body = substr($payload, 8);

        return match ($op) {
            self::OP_BOOTSTRAP => self::decodeBootstrap($body),
            self::OP_REQUEST => self::decodeRequest($body),
            default => null,
        };
    }

    /**
     * P4-D spike: `SELECT 1` via Rust async bridge (frame transport + `NUSA_ASYNC_IO=stub`).
     */
    public static function asyncSelectOne(): int
    {
        $result = self::asyncQuery('SELECT 1');
        if (!is_int($result)) {
            throw new \RuntimeException('async I/O: expected scalar for SELECT 1');
        }

        return $result;
    }

    /**
     * Run read-only SQL via Rust (`NUSA_EMBED_TRANSPORT=frame`, `NUSA_ASYNC_IO=stub`).
     *
     * @return int|array<int, array<string, mixed>>
     */
    public static function asyncQuery(string $sql): int|array
    {
        $inner = self::encodeHeader(self::OP_ASYNC_QUERY);
        $inner .= pack('V', strlen($sql));
        $inner .= $sql;
        self::writeLengthPrefixed($inner);

        while (true) {
            $payload = self::readLengthPrefixed();
            if ($payload === null || strlen($payload) < 8 || substr($payload, 0, 4) !== self::MAGIC) {
                throw new \RuntimeException('async I/O: invalid frame from parent');
            }
            $op = ord($payload[5]);
            $body = substr($payload, 8);
            if ($op === self::OP_ASYNC_RESULT) {
                return self::decodeAsyncResultBody($body);
            }
            if ($op === self::OP_ERROR) {
                $len = unpack('V', substr($body, 0, 4))[1];
                $msg = substr($body, 4, $len);
                throw new \RuntimeException('async I/O error: ' . $msg);
            }
            throw new \RuntimeException('async I/O: unexpected op ' . $op);
        }
    }

    /**
     * @param array<string, mixed> $message
     */
    public static function writeMessage(array $message): void
    {
        $type = $message['type'] ?? '';
        $inner = match ($type) {
            'Ack' => self::encodeHeader(self::OP_ACK),
            'Response' => self::encodeResponse($message),
            'Error' => self::encodeError($message),
            default => '',
        };
        if ($inner === '') {
            return;
        }
        self::writeLengthPrefixed($inner);
    }

    private static function readLengthPrefixed(): ?string
    {
        $header = fread(STDIN, 4);
        if ($header === false || strlen($header) < 4) {
            return null;
        }
        $length = unpack('V', $header)[1];
        if ($length === 0) {
            return '';
        }
        $payload = '';
        while (strlen($payload) < $length) {
            $chunk = fread(STDIN, $length - strlen($payload));
            if ($chunk === false) {
                return null;
            }
            $payload .= $chunk;
        }

        return $payload;
    }

    private static function writeLengthPrefixed(string $inner): void
    {
        fwrite(STDOUT, pack('V', strlen($inner)) . $inner);
        fflush(STDOUT);
    }

    private static function encodeHeader(int $op): string
    {
        return self::MAGIC . chr(self::VERSION) . chr($op) . "\0\0";
    }

    /**
     * @return array<string, mixed>
     */
    private static function decodeBootstrap(string $body): array
    {
        $len = unpack('V', substr($body, 0, 4))[1];
        $dir = substr($body, 4, $len);

        return ['type' => 'Bootstrap', 'code_dir' => $dir];
    }

    /**
     * @return array<string, mixed>
     */
    private static function decodeRequest(string $body): array
    {
        $off = 0;
        $methodLen = unpack('v', substr($body, $off, 2))[1];
        $off += 2;
        $uriLen = unpack('V', substr($body, $off, 4))[1];
        $off += 4;
        $hdrLen = unpack('V', substr($body, $off, 4))[1];
        $off += 4;
        $bodyLen = unpack('V', substr($body, $off, 4))[1];
        $off += 4;
        $method = substr($body, $off, $methodLen);
        $off += $methodLen;
        $uri = substr($body, $off, $uriLen);
        $off += $uriLen;
        $hdrJson = substr($body, $off, $hdrLen);
        $off += $hdrLen;
        $rawBody = substr($body, $off, $bodyLen);
        $headers = json_decode($hdrJson, true);
        if (!is_array($headers)) {
            $headers = [];
        }

        return [
            'type' => 'Request',
            'method' => $method,
            'uri' => $uri,
            'headers' => $headers,
            'body' => $rawBody,
            'cookies' => [],
        ];
    }

    /**
     * @param array<string, mixed> $message
     */
    private static function encodeResponse(array $message): string
    {
        $status = (int) ($message['status'] ?? 500);
        $headers = $message['headers'] ?? [];
        $body = (string) ($message['body'] ?? '');
        if (!is_array($headers)) {
            $headers = [];
        }
        $hdrJson = json_encode($headers, JSON_THROW_ON_ERROR);

        $inner = self::encodeHeader(self::OP_RESPONSE);
        $inner .= pack('v', $status);
        $inner .= pack('V', strlen($hdrJson));
        $inner .= pack('V', strlen($body));
        $inner .= $hdrJson . $body;

        return $inner;
    }

    /**
     * @param array<string, mixed> $message
     */
    private static function encodeError(array $message): string
    {
        $msg = (string) ($message['message'] ?? 'error');
        $inner = self::encodeHeader(self::OP_ERROR);
        $inner .= pack('V', strlen($msg));
        $inner .= $msg;

        return $inner;
    }

    /**
     * @return int|array<int, array<string, mixed>>
     */
    private static function decodeAsyncResultBody(string $body): int|array
    {
        $status = ord($body[0]);
        if ($status !== 0) {
            $len = unpack('V', substr($body, 1, 4))[1];
            $msg = substr($body, 5, $len);
            throw new \RuntimeException('async I/O failed: ' . $msg);
        }

        $rest = substr($body, 1);
        if (strlen($rest) === 8) {
            return (int) unpack('q', $rest)[1];
        }

        $kind = ord($rest[0]);
        if ($kind === 0) {
            return (int) unpack('q', substr($rest, 1, 8))[1];
        }
        if ($kind === 1) {
            $len = unpack('V', substr($rest, 1, 4))[1];
            $json = substr($rest, 5, $len);
            $decoded = json_decode($json, true);
            if (!is_array($decoded)) {
                throw new \RuntimeException('async I/O: invalid JSON rows');
            }

            return $decoded;
        }

        throw new \RuntimeException('async I/O: unknown result kind');
    }
}
