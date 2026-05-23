<?php

use Illuminate\Support\Facades\Route;

Route::get('/', static fn () => 'nusa-fixture-ok');
Route::get('/nusa-ping', static fn () => response('pong', 200));
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
