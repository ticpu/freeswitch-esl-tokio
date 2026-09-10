-- Loader for an execute_on_originate hook:
--
--   {execute_on_originate=lua <this file> <document> [content-type]}sofia/...
--
-- Reads the document and sets it as the channel's sip_multipart, so mod_sofia
-- carries it as a part of the INVITE body. Run through the `lua` API instead
-- (no session), it reports whether it could read the document, which proves
-- mod_lua is loaded and the path is readable by FreeSWITCH in its own mount
-- namespace and uid.
local path = argv[1]
local content_type = argv[2] or "application/pidf+xml"

local f, err = io.open(path, "r")
if not f then
  if stream then
    stream:write("cannot read " .. tostring(path) .. ": " .. tostring(err))
  else
    freeswitch.consoleLog("ERR", "load_multipart: cannot read " .. tostring(path) .. ": " .. tostring(err) .. "\n")
  end
  return
end
local body = f:read("*a")
f:close()

if stream then
  stream:write("ok " .. #body .. " bytes")
  return
end
session:setVariable("sip_multipart", content_type .. ":" .. body)
