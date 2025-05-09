import os

def format_size(bytes_size):
    if bytes_size >= 1024 * 1024:
        return f"{bytes_size / (1024 * 1024):.2f} MB"
    elif bytes_size >= 1024:
        return f"{bytes_size / 1024:.2f} KB"
    else:
        return f"{bytes_size} B"

def analyze_rs_file(filepath):
    code_lines = 0
    comment_lines = 0
    empty_lines = 0
    char_count = 0
    file_size = os.path.getsize(filepath)

    with open(filepath, 'r', encoding='utf-8') as f:
        for line in f:
            stripped = line.strip()
            char_count += len(line)
            if not stripped:
                empty_lines += 1
            elif stripped.startswith("//"):
                comment_lines += 1
            else:
                code_lines += 1

    total_lines = code_lines + comment_lines + empty_lines
    return code_lines, comment_lines, empty_lines, total_lines, char_count, file_size

def analyze_dir(directory):
    file_stats = []
    for root, _, files in os.walk(directory):
        for file in files:
            if file.endswith('.rs'):
                filepath = os.path.join(root, file)
                stats = analyze_rs_file(filepath)
                file_stats.append((filepath, *stats))
    return file_stats

def main():
    subprojects = ["cli", "core", "daemon"]
    grand_total = [0, 0, 0, 0, 0, 0]  # code, comment, empty, lines, chars, size

    for project in subprojects:
        if os.path.isdir(project):
            print(f"\n📁 Project: {project}")
            file_stats = analyze_dir(project)
            for filepath, code, comment, empty, lines, chars, size in file_stats:
                print(f"{filepath:<60} | Code: {code:4} | //: {comment:4} | Empty: {empty:4} "
                      f"| Total: {lines:4} | Chars: {chars:6} | Size: {format_size(size):>8}")
                grand_total[0] += code
                grand_total[1] += comment
                grand_total[2] += empty
                grand_total[3] += lines
                grand_total[4] += chars
                grand_total[5] += size
        else:
            print(f"{project} directory not found.")

    print("\n📊 Grand Total:")
    print(f"  Code lines:    {grand_total[0]}")
    print(f"  Comment lines: {grand_total[1]}")
    print(f"  Empty lines:   {grand_total[2]}")
    print(f"  Total lines:   {grand_total[3]}")
    print(f"  Characters:    {grand_total[4]}")
    print(f"  File size:     {format_size(grand_total[5])}")

if __name__ == "__main__":
    main()
