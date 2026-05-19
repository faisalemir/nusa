<?php

namespace NusaRs\OctaneDriver;

use Illuminate\Support\ServiceProvider;
use Laravel\Octane\Events\RequestReceived;
use Laravel\Octane\Events\RequestTerminated;
use Laravel\Octane\Events\WorkerStopping;
use Laravel\Octane\Octane;

class OctaneRustServiceProvider extends ServiceProvider
{
    /**
     * Register any application services.
     */
    public function register(): void
    {
        //
    }

    /**
     * Bootstrap Octane event listeners for state reset.
     */
    public function boot(): void
    {
        // Reset state on each request
        Octane::listen(RequestReceived::class, function ($event) {
            // Flush caches
            $event->sandbox->make('cache')->flush();
            $event->sandbox->make('session')->flush();
            
            // Clear database query log
            $event->sandbox->make('db')->flushQueryLog();
        });

        // Clean up on request termination
        Octane::listen(RequestTerminated::class, function ($event) {
            // Clean up any request-specific resources
        });

        // Clean up on worker stop
        Octane::listen(WorkerStopping::class, function () {
            // Close DB connections, flush logs, etc.
        });
    }
}
