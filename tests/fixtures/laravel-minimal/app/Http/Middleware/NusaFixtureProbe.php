<?php

declare(strict_types=1);

namespace App\Http\Middleware;

use Closure;
use Illuminate\Http\Request;
use Symfony\Component\HttpFoundation\Response;

/**
 * Fixture-only middleware to verify the HTTP stack runs on the Octane worker path.
 */
final class NusaFixtureProbe
{
    public function handle(Request $request, Closure $next): Response
    {
        $request->attributes->set('nusa_fixture_middleware', '1');

        return $next($request);
    }
}
