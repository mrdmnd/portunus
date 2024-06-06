import subprocess
import shutil
import os

# Clear existing schema contents
if os.path.exists("./game_state_schema"):
    shutil.rmtree("./game_state_schema")

# Compile the flatbuffer schema:
subprocess.run(["flatc", "--lua", "schema.fbs"])

# Get the result of calling LS on that directory:
filenames = os.listdir("./game_state_schema")
print("Schema files: ")
for fn in filenames:
    print(fn)


## TODO: FUTURE ME: you need to topologically sort these dynamically.
## currently i just modify the TOC file whenever i have a new guy show up.
# Edit the TOC file:
#with open("PortunusExporter.toc", "r") as f:
#    contents = [l.rstrip() for l in f.readlines()]

#start_idx = contents.index("# BEGIN_SCHEMA")
#end_idx = contents.index("# END_SCHEMA")
#prefix = contents[:start_idx+1]
#suffix = contents[end_idx:]

#final_contents = prefix + ["game_state_schema/"+fn for fn in filenames] + suffix

# Edit the TOC appropriately.
#with open("PortunusExporter.toc", "w") as f:
#    f.write("\n".join(final_contents))


# Next, edit the actual contents of the generated schema files to account for the fact that we're working in an environment without `require`.
header = """-- This sub code was programmatically added by update_flatbuffers.py
-- It is intended to replace the `require` functionality missing from the WOW lua environment.
-- We wrap the entire module in an function called "export_fn()" and then load that fn into Portunus.Modules at the bottom of this file.
local _, Portunus = ...
local function require(m) local e=Portunus.Modules[m] if e==nil then error("Failed to load module " .. m) end return e end
local function export_fn()

"""

footer = """
end
-- The above `end` keyword and the following line are designed to replace the `require` functionality missing from the WOW lua environment.
Portunus.Modules["{}"]=export_fn()
"""

for filename in os.listdir("./game_state_schema"):
    full_path = os.path.join("game_state_schema", filename)
    with open(full_path, "r") as fd:
        data = fd.read()
    with open(full_path, "w+") as fd:
        fd.write(header)
        fd.write(data)
        fd.write(footer.format("game_state_schema." + os.path.splitext(os.path.basename(filename))[0] ))
