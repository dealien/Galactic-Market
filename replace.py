with open(".agent/learning/rust.md", "r") as f:
    text = f.read()

import re

text = re.sub(r"<<<<<<< HEAD\n(.*?)\n=======\n(.*?)>>>>>>> origin/main\n", r"\1\n\n\2\n", text, flags=re.DOTALL)

with open(".agent/learning/rust.md", "w") as f:
    f.write(text)
