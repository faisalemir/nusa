<?php
/**
 * Nusa Octane Worker — bridges Laravel to the Nusa Rust orchestrator.
 *
 * Handles the IPC protocol: receives framed JSON/MessagePack requests,
 * dispatches through the Laravel kernel, and sends framed responses.
 *
 * @package Nusa\Octane
 */

declare(strict_types=1);

namespace Nusa\Octane;

use Illuminate\Contracts\Http\Kernel;
use Illuminate\Http\Request;
use Illuminate\Support\Facades\Facade;

class Worker
{
    /** @var string */
    private string $socketPath;

    /** @var Kernel */
    private Kernel $kernel;

    /** @var resource|null */
    private $socket;

    public function __construct(string $socketPath, Kernel $kernel)
    {
        $this->socketPath = $socketPath;
        $this->kernel = $kernel;
    }

    /**
     * Main IPC loop — accepts connections and processes requests.
     */
    public function run(): void
    {
        if (file_exists($this->socketPath)) {
            unlink($this->socketPath);
        }

        $this->socket = stream_socket_server(
            'unix://' . $this->socketPath,
            $errno,
            $errstr,
            STREAM_SERVER_BIND | STREAM_SERVER_LISTEN
        );

        if (!$this->socket) {
            fwrite(STDERR, "Failed to create socket: $errstr ($errno)\n");
            exit(1);
        }

        fwrite(STDERR, "Nusa Octane worker listening on {$this->socketPath}\n");

        while (true) {
            $conn = @stream_socket_accept($this->socket, -1);
            if (!$conn) {
                continue;
            }

            $this->handleConnection($conn);
            fclose($conn);
        }
    }

    /**
     * Handle a single orchestrator connection.
     */
    private function handleConnection($conn): void
    {
        // Read Hello handshake
        $hello = $this->readFrame($conn);
        if (!$hello) {
            return;
        }

        $message = json_decode($hello, true);
        if (!isset($message['type']) || $message['type'] !== 'Hello') {
            return;
        }

        // Send Ack
        $this->writeFrame($conn, json_encode(['type' => 'Ack']));

        // Process requests
        while (true) {
            $frame = $this->readFrame($conn);
            if ($frame === false || $frame === '') {
                break;
            }

            $request = json_decode($frame, true);
            if (!isset($request['type']) || $request['type'] !== 'Request') {
                continue;
            }

            $response = $this->handleRequest($request);
            $this->writeFrame($conn, json_encode($response));
        }
    }

    /**
     * Handle an HTTP request dispatched from the orchestrator.
     */
    private function handleRequest(array $request): array
    {
        $method = $request['method'] ?? 'GET';
        $uri = $request['uri'] ?? '/';
        $headers = $request['headers'] ?? [];
        $body = $request['body'] ?? '';

        // Build Laravel request with superglobals emulation
        $_SERVER['REQUEST_METHOD'] = $method;
        $_SERVER['REQUEST_URI'] = $uri;
        $_SERVER['HTTP_HOST'] = $headers['Host'][0] ?? 'localhost';

        foreach ($headers as $name => $values) {
            $key = 'HTTP_' . strtoupper(str_replace('-', '_', $name));
            $_SERVER[$key] = is_array($values) ? implode(', ', $values) : $values;
        }

        $laravelRequest = Request::create($uri, $method, [], [], [], [], $body);

        // Handle the request through the kernel
        $symfonyResponse = $this->kernel->handle($laravelRequest);

        // Convert to IPC response format
        $responseBody = (string) $symfonyResponse->getContent();
        $responseHeaders = [];
        foreach ($symfonyResponse->headers->all() as $name => $values) {
            $responseHeaders[$name] = $values;
        }

        return [
            'type' => 'Response',
            'id' => $request['id'],
            'status' => $symfonyResponse->getStatusCode(),
            'headers' => $responseHeaders,
            'body' => $responseBody,
            'terminated' => false,
        ];
    }

    /**
     * Read a length-prefixed frame from the socket.
     */
    private function readFrame($conn): string|false
    {
        // Read 4-byte little-endian length prefix
        $header = fread($conn, 4);
        if ($header === false || strlen($header) < 4) {
            return false;
        }

        $length = unpack('V', $header)[1];

        // Read payload
        $payload = '';
        while (strlen($payload) < $length) {
            $chunk = fread($conn, $length - strlen($payload));
            if ($chunk === false) {
                return false;
            }
            $payload .= $chunk;
        }

        return $payload;
    }

    /**
     * Write a length-prefixed frame to the socket.
     */
    private function writeFrame($conn, string $data): void
    {
        $length = strlen($data);
        $header = pack('V', $length);
        fwrite($conn, $header . $data);
    }

    /**
     * Clean up on shutdown.
     */
    public function __destruct()
    {
        if (is_resource($this->socket)) {
            fclose($this->socket);
        }
        if (file_exists($this->socketPath)) {
            @unlink($this->socketPath);
        }
    }
}
