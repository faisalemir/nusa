<?php
/**
 * Nusa Octane Service Provider.
 *
 * Registers Nusa runtime as an Octane server, wires up Octane event
 * listeners for state reset, and configures the driver for Laravel.
 *
 * @package Nusa\Octane
 */

declare(strict_types=1);

namespace Nusa\Octane;

use Illuminate\Support\Facades\Facade;
use Illuminate\Support\ServiceProvider;
use Laravel\Octane\Events\RequestHandled;
use Laravel\Octane\Events\RequestReceived;
use Laravel\Octane\Events\RequestTerminated;
use Laravel\Octane\Events\WorkerErrorOccurred;
use Laravel\Octane\Events\WorkerStarting;
use Laravel\Octane\Events\WorkerStopping;

class NusaOctaneServiceProvider extends ServiceProvider
{
    /**
     * Bootstrap the Nusa Octane service.
     */
    public function boot(): void
    {
        // Publish Nusa Octane configuration
        $this->publishes([
            __DIR__ . '/../config/nusa-octane.php' => config_path('nusa-octane.php'),
        ], 'nusa-config');

        // Register Octane event listeners for state reset
        $this->app['events']->listen(WorkerStarting::class, function ($event) {
            // Worker started — reset any persistent state
            $this->resetWorkerState();
        });

        $this->app['events']->listen(RequestReceived::class, function ($event) {
            // Flush per-request caches before handling
            $this->clearResolvedInstances();
        });

        $this->app['events']->listen(RequestHandled::class, function ($event) {
            // Post-request cleanup
        });

        $this->app['events']->listen(RequestTerminated::class, function ($event) {
            // Clear resolved facade instances
            $this->clearResolvedInstances();
        });

        $this->app['events']->listen(WorkerErrorOccurred::class, function ($event) {
            // Error recovery — don't let state leak
            $this->resetWorkerState();
        });

        $this->app['events']->listen(WorkerStopping::class, function ($event) {
            // Final worker cleanup
            $this->resetWorkerState();
        });
    }

    /**
     * Register Nusa Octane bindings.
     */
    public function register(): void
    {
        $this->mergeConfigFrom(
            __DIR__ . '/../config/nusa-octane.php',
            'nusa-octane'
        );
    }

    /**
     * Clear resolved facade instances to prevent state bleed.
     */
    protected function clearResolvedInstances(): void
    {
        // Clear commonly cached instances
        Facade::clearResolvedInstances();

        // Reset application-level singletons
        $this->app->forgetInstance('router');
        $this->app->forgetInstance('url');
        $this->app->forgetInstance('redirect');
        $this->app->forgetInstance('cookie');
        $this->app->forgetInstance('session.store');
        $this->app->forgetInstance('auth');
        $this->app->forgetInstance('cache');
    }

    /**
     * Full worker state reset for clean request handling.
     */
    protected function resetWorkerState(): void
    {
        $this->clearResolvedInstances();

        // Clear any static caches that packages may have set
        if (class_exists(\Illuminate\Support\ServiceProvider::class)) {
            \Illuminate\Support\ServiceProvider::clearResolvedInstances();
        }
    }
}
