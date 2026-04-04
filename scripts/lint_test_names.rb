#!/usr/bin/env ruby
# frozen_string_literal: true

ROOT = File.expand_path("..", __dir__)
DEFAULT_PATHS = [
  "src",
  "tests",
].freeze

# This lint only checks mechanically detectable anti-patterns.
# Category-specific naming still relies on local rules and review.
def rust_files(paths)
  patterns = paths.flat_map do |path|
    absolute = File.expand_path(path, ROOT)
    if File.directory?(absolute)
      [File.join(absolute, "**/*.rs")]
    else
      [absolute]
    end
  end

  patterns.flat_map { |pattern| Dir.glob(pattern) }.sort.uniq
end

def starts_char_literal?(line, index)
  return false unless line[index] == "'"

  if line[index + 1] == "\\"
    closing = line.index("'", index + 2)
    return !closing.nil?
  end

  line[index + 2] == "'"
end

def count_braces(line, state)
  opens = 0
  closes = 0
  index = 0

  while index < line.length
    char = line[index]

    if state[:in_block_comment].positive?
      if char == "/" && line[index + 1] == "*"
        state[:in_block_comment] += 1
        index += 2
        next
      end

      if char == "*" && line[index + 1] == "/"
        state[:in_block_comment] -= 1
        index += 2
        next
      end

      index += 1
      next
    end

    if state[:in_single]
      if state[:escaped]
        state[:escaped] = false
        index += 1
        next
      end

      state[:escaped] = true if char == "\\"
      state[:in_single] = false if char == "'"
      index += 1
      next
    end

    if state[:in_double]
      if state[:escaped]
        state[:escaped] = false
        index += 1
        next
      end

      state[:escaped] = true if char == "\\"
      state[:in_double] = false if char == "\""
      index += 1
      next
    end

    if state[:in_raw]
      if char == "\""
        terminator = "\"" + ("#" * state[:raw_hashes])
        if line[index, terminator.length] == terminator
          state[:in_raw] = false
          state[:raw_hashes] = 0
          index += terminator.length
          next
        end
      end

      index += 1
      next
    end

    break if char == "/" && line[index + 1] == "/"

    if char == "/" && line[index + 1] == "*"
      state[:in_block_comment] = 1
      index += 2
      next
    end

    if char == "r"
      hashes = 0
      probe = index + 1
      while line[probe] == "#"
        hashes += 1
        probe += 1
      end

      if line[probe] == "\""
        state[:in_raw] = true
        state[:raw_hashes] = hashes
        index = probe + 1
        next
      end
    end

    case char
    when "'"
      state[:in_single] = true if starts_char_literal?(line, index)
    when "\""
      state[:in_double] = true
    when "{"
      opens += 1
    when "}"
      closes += 1
    end

    index += 1
  end

  [opens, closes]
end

def normalized_mod_name(name)
  return nil if name.nil?
  return nil if name == "tests"

  normalized = name.sub(/_tests?\z/, "")
  return nil if normalized.empty?

  normalized
end

paths = ARGV.empty? ? DEFAULT_PATHS : ARGV
errors = []

rust_files(paths).each do |file|
  rel = file.delete_prefix("#{ROOT}/")
  lines = File.readlines(file, chomp: true)

  brace_depth = 0
  brace_state = {
    in_block_comment: 0,
    in_single: false,
    in_double: false,
    in_raw: false,
    raw_hashes: 0,
    escaped: false,
  }
  mod_stack = []
  pending_test_attr = false
  seen_test_names = {}

  lines.each_with_index do |line, idx|
    stripped = line.strip
    line_no = idx + 1

    while mod_stack.any? && brace_depth < mod_stack.last[:depth]
      mod_stack.pop
    end

    if (match = stripped.match(/^mod\s+([a-zA-Z0-9_]+)\s*\{/))
      mod_stack << { name: match[1], depth: brace_depth + 1 }
    end

    if stripped.match?(/^#\[(?:test|rstest|tokio::test)\b/) || (pending_test_attr && stripped.start_with?("#["))
      pending_test_attr = true
    elsif pending_test_attr && (match = stripped.match(/^(?:async\s+)?fn\s+([a-zA-Z0-9_]+)\s*\(/))
      test_name = match[1]

      if test_name.include?("returns_expected")
        errors << "#{rel}:#{line_no} test name must not contain `returns_expected`: #{test_name} (use a concrete result, drop the suffix if the result is already implied, or split the rstest if cases do not share one guarantee)"
      end

      mod_path = mod_stack.map { |entry| entry[:name] }.join("::")
      scoped_name = mod_path.empty? ? test_name : "#{mod_path}::#{test_name}"

      if (previous = seen_test_names[scoped_name])
        errors << "#{rel}:#{line_no} duplicate test name in same module: #{test_name} (first seen at line #{previous})"
      else
        seen_test_names[scoped_name] = line_no
      end

      mod_name = normalized_mod_name(mod_stack.last&.dig(:name))
      if mod_name && test_name.start_with?("#{mod_name}_")
        errors << "#{rel}:#{line_no} redundant module prefix `#{mod_name}_` in test name: #{test_name}"
      end

      pending_test_attr = false
    elsif pending_test_attr && !stripped.empty? && !stripped.start_with?("//")
      pending_test_attr = false
    end

    opens, closes = count_braces(line, brace_state)
    brace_depth += opens - closes
  end
end

if errors.empty?
  puts "test-name lint passed"
else
  warn "test-name lint failed:"
  errors.each { |error| warn "- #{error}" }
  exit 1
end
