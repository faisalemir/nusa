<?php

declare(strict_types=1);

namespace Nusa\Octane\Embed\Database;

use Illuminate\Database\SQLiteConnection;
use Nusa\Octane\Embed\AsyncIo;
use Nusa\Octane\Embed\FrameCodec;

/**
 * Routes read-only, unbound `SELECT` queries through embed NEB1 async I/O (P4-D spike).
 */
final class NusaAsyncSqliteConnection extends SQLiteConnection
{
    /**
     * @param  array<int, mixed>  $bindings
     * @return array<int, object>
     */
    public function select($query, $bindings = [], $useReadPdo = true): array
    {
        if (
            AsyncIo::enabled()
            && $bindings === []
            && AsyncIo::isReadonlySql($query)
        ) {
            return $this->selectViaAsync($query);
        }

        return parent::select($query, $bindings, $useReadPdo);
    }

    /**
     * @return array<int, object>
     */
    private function selectViaAsync(string $query): array
    {
        $result = FrameCodec::asyncQuery(trim($query));

        if (is_int($result)) {
            return [(object) ['v' => $result]];
        }

        $rows = [];
        foreach ($result as $row) {
            if (!is_array($row)) {
                continue;
            }
            $rows[] = (object) $row;
        }

        return $rows;
    }
}
