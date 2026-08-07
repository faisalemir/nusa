<?php

use Illuminate\Support\Facades\Route;

Route::get('/', static fn () => 'nusa-fixture-ok');
Route::get('/nusa-ping', static fn () => response('pong', 200));
Route::get('/nusa-db-ping', static function () {
    $row = \Illuminate\Support\Facades\DB::selectOne('select 1 as one');

    return response('db:' . (string) ($row->one ?? ''), 200);
});
Route::get('/nusa-db-async-proxy', static function () {
    if (!\Nusa\Octane\Embed\AsyncIo::enabled()) {
        return response('proxy:disabled', 200);
    }

    $row = \Illuminate\Support\Facades\DB::selectOne('SELECT 2 AS two');

    return response('proxy:' . (string) ($row->two ?? ''), 200);
});
Route::get('/nusa-async-spike', static function () {
    if (getenv('NUSA_ASYNC_IO') !== 'stub' || getenv('NUSA_EMBED_TRANSPORT') !== 'frame') {
        return response('async:disabled', 200);
    }

    $value = \Nusa\Octane\Embed\FrameCodec::asyncSelectOne();

    return response('async:' . $value, 200);
});
Route::get('/nusa-async-sql', static function () {
    if (getenv('NUSA_ASYNC_IO') !== 'stub' || getenv('NUSA_EMBED_TRANSPORT') !== 'frame') {
        return response('async:disabled', 200);
    }

    $sql = request()->query('sql', 'SELECT 2 AS two');
    if (!is_string($sql) || $sql === '') {
        return response('async:bad-request', 400);
    }

    $result = \Nusa\Octane\Embed\FrameCodec::asyncQuery($sql);

    return response('async:' . json_encode($result, JSON_THROW_ON_ERROR), 200, [
        'Content-Type' => 'application/json',
    ]);
});
Route::post('/nusa-echo', static function () {
    $body = request()->getContent();
    return response('echo:' . $body, 200);
});
Route::get('/nusa-query', static fn () => 'q=' . request()->query('q', ''));
Route::get('/nusa-counter', static function () {
    static $n = 0;
    $n++;

    return 'count:' . $n;
});
Route::get('/nusa-session-set', static function () {
    session(['nusa_token' => 'fixture-session-ok']);

    return response('session-set', 200);
});
Route::get('/nusa-session-get', static function () {
    return 'session:' . session('nusa_token', 'missing');
});
Route::get('/nusa-middleware', static function () {
    $flag = request()->attributes->get('nusa_fixture_middleware', '0');

    return 'mw:' . $flag;
});
