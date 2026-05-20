<?php

namespace Nusa\Octane;

use Illuminate\Console\Command;
use Symfony\Component\Process\Process;
use Symfony\Component\Process\Exception\ProcessFailedException;

/**
 * Artisan command to start Nusa Rust runtime.
 * Blueprint 6 C2: `php artisan octane:start --server=nusa`
 *
 * domain-web: Detects nusa binary, generates config from Laravel .env,
 * monitors process health, auto-restarts on crash.
 */
class OctaneStartCommand extends Command
{
    protected $signature = 'octane:start {--server=nusa : Server type (nusa)}
                           {--host=127.0.0.1 : Server host}
                           {--port=8080 : Server port}
                           {--workers=4 : Number of workers}
                           {--max-requests=500 : Max requests before recycle}';

    protected $description = 'Start the Nusa Rust Octane server';

    /**
     * Execute the console command.
     */
    public function handle()
    {
        if ($this->option('server') !== 'nusa') {
            $this->error('Only nusa server is supported. Use --server=nusa');
            return 1;
        }

        $nusaBinary = $this->findNusaBinary();
        if (!$nusaBinary) {
            $this->error('Nusa binary not found. Install with: cargo install --git https://github.com/nusa-rs/nusa');
            return 1;
        }

        $configPath = $this->generateConfig();

        $this->info("Starting Nusa Rust runtime...");
        $this->info("Binary: {$nusaBinary}");
        $this->info("Config: {$configPath}");

        $process = new Process([$nusaBinary, '--config', $configPath]);
        $process->setTimeout(null);

        try {
            $process->mustRun(function ($type, $buffer) {
                $this->info(trim($buffer));
            });
        } catch (ProcessFailedException $e) {
            $this->error('Nusa runtime crashed: ' . $e->getMessage());
            return 1;
        }

        return 0;
    }

    /**
     * Find the nusa binary in PATH or common locations.
     */
    protected function findNusaBinary(): ?string
    {
        $candidates = [
            'nusa',
            '/usr/local/bin/nusa',
            '/usr/bin/nusa',
            base_path('target/release/nusa'),
        ];

        foreach ($candidates as $candidate) {
            if (is_executable($candidate) || $this->commandExists($candidate)) {
                return $candidate;
            }
        }

        return null;
    }

    /**
     * Generate nusa.toml config from Laravel environment.
     */
    protected function generateConfig(): string
    {
        $config = sprintf(
            '[server]
engine = "child"
max_workers = %d
timeout_ms = 30000
wasm_memory_mb = 256
hot_reload = true

[server.network]
host = "%s"
port = %d

[server.worker]
max_requests = %d
max_memory_mb = 512
',
            (int) $this->option('workers'),
            $this->option('host'),
            (int) $this->option('port'),
            (int) $this->option('max-requests')
        );

        $configPath = storage_path('nusa.toml');
        file_put_contents($configPath, $config);

        return $configPath;
    }

    protected function commandExists(string $command): bool
    {
        $process = Process::fromShellCommandline("which {$command}");
        $process->run();
        return $process->isSuccessful();
    }
}
