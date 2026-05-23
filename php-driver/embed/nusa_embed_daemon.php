<?php

declare(strict_types=1);

/**
 * Long-lived embed worker over stdin/stdout.
 *
 * Transports:
 *   - frame (default): binary `NEB1` (P4-B spike)
 *   - JSON: `NUSA_EMBED_TRANSPORT=json` — length-prefixed JSON (dev fallback)
 *
 * @package Nusa\Octane
 */

require_once __DIR__ . '/nusa_embed.php';

use Nusa\Octane\Embed\FrameCodec;

$useFrame = getenv('NUSA_EMBED_TRANSPORT') !== 'json';

/**
 * @return array<string, mixed>|null
 */
function read_message(): ?array
{
    global $useFrame;
    if ($useFrame) {
        return FrameCodec::readMessage();
    }

    $header = fread(STDIN, 4);
    if ($header === false || strlen($header) < 4) {
        return null;
    }
    $length = unpack('V', $header)[1];
    if ($length === 0) {
        return [];
    }
    $payload = '';
    while (strlen($payload) < $length) {
        $chunk = fread(STDIN, $length - strlen($payload));
        if ($chunk === false) {
            return null;
        }
        $payload .= $chunk;
    }
    $decoded = json_decode($payload, true);

    return is_array($decoded) ? $decoded : null;
}

/**
 * @param array<string, mixed> $data
 */
function write_message(array $data): void
{
    global $useFrame;
    if ($useFrame) {
        FrameCodec::writeMessage($data);

        return;
    }

    $json = json_encode($data, JSON_THROW_ON_ERROR);
    fwrite(STDOUT, pack('V', strlen($json)) . $json);
    fflush(STDOUT);
}

$bootstrapped = false;

while (true) {
    $message = read_message();
    if ($message === null) {
        break;
    }

    $type = $message['type'] ?? '';

    if ($type === 'Bootstrap') {
        $root = $message['code_dir'] ?? getenv('NUSA_CODE_DIR') ?: getcwd();
        if (!is_string($root) || $root === '') {
            write_message(['type' => 'Error', 'message' => 'missing code_dir']);
            continue;
        }
        putenv('NUSA_CODE_DIR=' . $root);
        $autoload = $root . '/vendor/autoload.php';
        if (!is_file($autoload)) {
            write_message(['type' => 'Error', 'message' => 'vendor/autoload.php missing']);
            continue;
        }
        require_once $autoload;
        nusa_embed_ensure_driver_loaded($root);
        \Nusa\Octane\Embed\Runtime::bootstrap($root);
        $bootstrapped = true;
        write_message(['type' => 'Ack']);
        continue;
    }

    if ($type === 'Request') {
        if (!$bootstrapped) {
            write_message([
                'type' => 'Response',
                'status' => 503,
                'headers' => ['Content-Type' => ['text/plain']],
                'body' => 'worker not bootstrapped',
            ]);
            continue;
        }
        write_message(\Nusa\Octane\Embed\Runtime::handleRequest($message));
        \Nusa\Octane\Embed\Runtime::resetRequestState();
        continue;
    }

    if ($type === 'Ping') {
        write_message(['type' => 'Pong']);
    }
}
