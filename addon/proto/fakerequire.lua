local _, ADDONSELF = ...
ADDONSELF.luapb = {} -- init

ADDONSELF.luapb.require = function(modname)
	print("required called with modname " .. modname)
	if modname == 'bit' then
		return bit
	elseif modname == 'struct' then
		-- TODO
		return {}
	end

	modname = string.gsub(modname, '.*%.', '')
	return ADDONSELF.luapb[modname]
end
