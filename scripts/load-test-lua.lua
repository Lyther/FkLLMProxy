-- Lua script for wrk load testing
-- POST request with JSON body

wrk.method                   = "POST"
wrk.body                     = '{"model":"gemini-2.5-flash","messages":[{"role":"user","content":"Hello"}],"max_tokens":10}'
wrk.headers["Content-Type"]  = "application/json"
wrk.headers["Authorization"] = "Bearer sk-vertex-bridge-dev"
