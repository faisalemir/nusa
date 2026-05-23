#!/usr/bin/env php
<?php
/**
 * Minimal stdin/stdout IPC bootstrap for Nusa child engine (Normal mode).
 * Reads length-prefixed JSON IpcMessage::Request, returns IpcMessage::Response.
 */

declare(strict_types=1);

$stdin = fopen('php://stdin', 'rb');
if ($stdin === false) {
    fwrite(STDERR, "stdin unavailable\n");
    exit(1);
}

$header = fread($stdin, 4);
if ($header === false || strlen($header) !== 4) {
    exit(0);
}

$len = unpack('V', $header)[1];
$payload = $len > 0 ? fread($stdin, $len) : '';
fclose($stdin);

if ($payload === false || $payload === '') {
    exit(0);
}

/** @var array<string, mixed> $request */
$request = json_decode($payload, true, 512, JSON_THROW_ON_ERROR);

if (($request['type'] ?? '') !== 'Request') {
    exit(0);
}

$uri = (string) ($request['uri'] ?? '/');
$body = 'nusa-static-ok';
if ($uri === '/nusa-echo') {
    $raw = $request['body'] ?? '';
    if (is_array($raw)) {
        $bytes = '';
        foreach ($raw as $byte) {
            if (is_int($byte) || is_numeric($byte)) {
                $bytes .= chr((int) $byte);
            }
        }
        $body = 'echo:' . $bytes;
    } elseif (is_string($raw)) {
        $body = 'echo:' . $raw;
    }
}

$response = [
    'type' => 'Response',
    'id' => $request['id'],
    'status' => 200,
    'headers' => ['Content-Type' => ['text/plain; charset=UTF-8']],
    'body' => $body,
    'terminated' => false,
];

$json = json_encode($response, JSON_THROW_ON_ERROR);
$frame = pack('V', strlen($json)) . $json;
fwrite(STDOUT, $frame);
