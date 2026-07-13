@echo off
REM Database-dependent test commands (requires PostgreSQL + Redis running)
REM
REM Usage: set REQUIRE_DB_TESTS=1 then run any of:
REM
REM   cargo test -p db --test schema
REM   cargo test -p server --test conversations
REM   cargo test -p server --test rbac
REM   cargo test
REM
REM Example:
REM   set REQUIRE_DB_TESTS=1 && cargo test -p server --test conversations
REM
REM See specs/013-conversation-core/quickstart.md for full details.
echo See specs/013-conversation-core/quickstart.md for backend test commands.
echo.
echo Available:
echo   REQUIRE_DB_TESTS=1 cargo test -p db --test schema
echo   REQUIRE_DB_TESTS=1 cargo test -p server --test conversations
echo   REQUIRE_DB_TESTS=1 cargo test -p server --test rbac
echo   REQUIRE_DB_TESTS=1 cargo test
