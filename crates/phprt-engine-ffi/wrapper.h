/**
 * Wrapper header for PHP ZTS embed bindings.
 *
 * Include only the headers needed for php_embed mode.
 * This file is consumed by bindgen to generate Rust FFI bindings.
 */

#ifdef __linux__
#include <main/php.h>
#include <main/SAPI.h>
#include <main/php_main.h>
#include <main/php_embed.h>
#include <Zend/zend.h>
#endif
