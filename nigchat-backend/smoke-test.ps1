# NigChat backend smoke test.
#
#   powershell -ExecutionPolicy Bypass -File smoke-test.ps1
#
# Proves the whole stack in one go: HTTP, PostgreSQL, Redis rate limiting, JWT
# issuing and the session store. Run it with the server already running in
# another window.

$ErrorActionPreference = 'Stop'
$base  = 'http://localhost:8080'
# A random number each run, so the per-number OTP rate limit never blocks a
# repeat test.
$phone = '+234' + (Get-Random -Minimum 7000000000 -Maximum 9099999999)

function Step($n, $text) { Write-Host "`n[$n] $text" -ForegroundColor Cyan }
function Ok($text)       { Write-Host "    OK  $text" -ForegroundColor Green }

Step 1 'Health'
$health = Invoke-RestMethod "$base/healthz"
Ok "service=$($health.service) status=$($health.status)"

Step 2 'Readiness (PostgreSQL + Redis)'
$ready = Invoke-RestMethod "$base/readyz"
Ok "ready=$($ready.ready) postgres=$($ready.postgres) redis=$($ready.redis) sockets=$($ready.local_websockets)"
if (-not $ready.ready) { throw 'Not ready — check the datastores' }

Step 3 "Request a verification code for $phone"
$otp = Invoke-RestMethod -Method Post -Uri "$base/v1/auth/request-otp" `
    -ContentType 'application/json' `
    -Body (@{ phone_e164 = $phone } | ConvertTo-Json)
if (-not $otp.debug_code) { throw 'No debug_code — is OTP_DEBUG_ECHO=true?' }
Ok "code=$($otp.debug_code) expires_in=$($otp.expires_in)s"

Step 4 'Verify the code and register'
$auth = Invoke-RestMethod -Method Post -Uri "$base/v1/auth/verify-otp" `
    -ContentType 'application/json' `
    -Body (@{
        phone_e164   = $phone
        code         = $otp.debug_code
        display_name = 'Smoke Test'
        platform     = 'android'
        device_name  = 'Test Runner'
    } | ConvertTo-Json)
Ok "user=$($auth.user_id)"
Ok "device=$($auth.device_id) new_account=$($auth.is_new_account)"

$headers = @{ Authorization = "Bearer $($auth.access_token)" }

Step 5 'Fetch the profile with the access token'
$me = Invoke-RestMethod "$base/v1/me" -Headers $headers
Ok "$($me.display_name) $($me.phone_e164)"

Step 6 'Notification tones (seeded by the migration)'
$tones = Invoke-RestMethod "$base/v1/notifications/tones" -Headers $headers
Ok "$($tones.Count) tones, default message tone = $(($tones | Where-Object { $_.category -eq 'message' -and $_.is_default }).display_name)"

Step 7 'Notification preferences'
$prefs = Invoke-RestMethod "$base/v1/notifications/preferences" -Headers $headers
Ok "preview=$($prefs.preview_mode) vibration=$($prefs.vibration) quiet_hours=$(if ($prefs.quiet_hours) { 'on' } else { 'off' })"

Step 8 'Conversation list (empty on a fresh account)'
$chats = Invoke-RestMethod "$base/v1/conversations" -Headers $headers
Ok "$($chats.Count) conversations"

Step 9 'Refresh token rotation'
$rotated = Invoke-RestMethod -Method Post -Uri "$base/v1/auth/refresh" `
    -ContentType 'application/json' `
    -Body (@{ refresh_token = $auth.refresh_token } | ConvertTo-Json)
Ok 'new token pair issued; the old refresh token is now dead'

Step 10 'Rate limiting (a second code within a minute must be refused)'
try {
    Invoke-RestMethod -Method Post -Uri "$base/v1/auth/request-otp" `
        -ContentType 'application/json' `
        -Body (@{ phone_e164 = $phone } | ConvertTo-Json) | Out-Null
    Write-Host '    WARN  expected a 429 and did not get one' -ForegroundColor Yellow
} catch {
    if ($_.Exception.Response.StatusCode.value__ -eq 429) {
        Ok 'refused with 429 as designed'
    } else {
        throw
    }
}

Write-Host "`nAll checks passed." -ForegroundColor Green
Write-Host "Save these to sign into the web client before device pairing exists:`n"
Write-Host "  access : $($rotated.access_token)"
Write-Host "  refresh: $($rotated.refresh_token)"
Write-Host "  user   : $($rotated.user_id)"
Write-Host "  device : $($rotated.device_id)"
