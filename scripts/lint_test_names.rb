#!/usr/bin/env ruby
# frozen_string_literal: true

ROOT = File.expand_path("..", __dir__)
DEFAULT_PATHS = [
  "src",
  "tests",
].freeze

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

def count_braces(line)
  opens = 0
  closes = 0
  in_single = false
  in_double = false
  escaped = false

  line.each_char do |char|
    if escaped
      escaped = false
      next
    end

    if in_single
      escaped = true if char == "\\"
      in_single = false if char == "'"
      next
    end

    if in_double
      escaped = true if char == "\\"
      in_double = false if char == "\""
      next
    end

    case char
    when "#"
      break
    when "'"
      in_single = true
    when "\""
      in_double = true
    when "{"
      opens += 1
    when "}"
      closes += 1
    end
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
    elsif pending_test_attr && (match = stripped.match(/^fn\s+([a-zA-Z0-9_]+)\s*\(/))
      test_name = match[1]

      if test_name.include?("returns_expected")
        errors << "#{rel}:#{line_no} test name must not contain `returns_expected`: #{test_name} (use a concrete result, drop the suffix if the result is already implied, or split the rstest if cases do not share one guarantee)"
      end

      if (previous = seen_test_names[test_name])
        errors << "#{rel}:#{line_no} duplicate test name in same file: #{test_name} (first seen at line #{previous})"
      else
        seen_test_names[test_name] = line_no
      end

      mod_name = normalized_mod_name(mod_stack.last&.dig(:name))
      if mod_name && test_name.start_with?("#{mod_name}_")
        errors << "#{rel}:#{line_no} redundant module prefix `#{mod_name}_` in test name: #{test_name}"
      end

      pending_test_attr = false
    elsif pending_test_attr && !stripped.empty? && !stripped.start_with?("//")
      pending_test_attr = false
    end

    opens, closes = count_braces(line)
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
