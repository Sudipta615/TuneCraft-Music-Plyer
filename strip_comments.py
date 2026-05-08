import os
import re

# Regex to match:
# 1. Optional preceding empty comment lines: (?:^[ \t]*(?:///|//)[ \t]*\n)*
# 2. Trigger line starting with Fix/Issue/Security/Bug: ^[ \t]*(?:///|//)[ \t]*(?:Fix|Issue|Security|Bug)\b.*
# 3. Optional subsequent comment lines that belong to the same block: (?:\n[ \t]*(?:///|//).*)*\n?
regex = re.compile(r'(?m)(?:^[ \t]*(?:///|//)[ \t]*\n)*^[ \t]*(?:///|//)[ \t]*(?:Fix|Issue|Security|Bug)\b.*(?:\n[ \t]*(?:///|//).*)*\n?')

def process_file(filepath):
    with open(filepath, 'r', encoding='utf-8') as f:
        content = f.read()

    new_content = regex.sub('', content)

    if new_content != content:
        with open(filepath, 'w', encoding='utf-8') as f:
            f.write(new_content)
        print(f"Cleaned comments in: {filepath}")

def main():
    crates_dir = 'crates'
    for root, dirs, files in os.walk(crates_dir):
        for file in files:
            if file.endswith('.rs'):
                filepath = os.path.join(root, file)
                process_file(filepath)

if __name__ == '__main__':
    main()
