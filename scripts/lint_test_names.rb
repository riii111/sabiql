#!/usr/bin/env ruby
# frozen_string_literal: true

require "strscan"
require "set"

ROOT = File.expand_path("..", __dir__)
DEFAULT_PATHS = ["src", "tests"].freeze

def rust_files(paths)
  patterns = paths.flat_map do |path|
    absolute = File.expand_path(path, ROOT)
    File.directory?(absolute) ? File.join(absolute, "**/*.rs") : absolute
  end

  patterns.flat_map { |pattern| Dir.glob(pattern) }.sort.uniq
end

# Keep Rust token trees balanced without interpreting Rust expressions or expanding macros.
class RustTokens
  IDENTIFIER = /(?:r\#)?[_\p{XID_Start}][\p{XID_Continue}]*/u
  CHAR_LITERAL = /'(?:\\(?:u\{[\da-fA-F_]+\}|x[\da-fA-F]{2}|[^\r\n])|[^'\\\r\n])'/
  STRING_ESCAPE = /\\(?:u\{([\da-fA-F_]+)\}|x([\da-fA-F]{2})|(\r?\n\s*)|(.))/m
  ESCAPED_CHARACTERS = { "0" => "\0", "n" => "\n", "r" => "\r", "t" => "\t" }.freeze
  GROUP_END = { "(" => ")", "[" => "]", "{" => "}" }.freeze
  Token = Struct.new(:text, :line, :after_group, :literal)

  def initialize(source)
    @tokens = []
    @scanner = StringScanner.new(source)
    line = 1
    groups = []

    until @scanner.eos?
      if @scanner.scan(%r{\s+|//[^\n]*})
        line += @scanner.matched.count("\n")
      elsif @scanner.scan(%r{/\*})
        line += skip_block_comment
      else
        start = @scanner.pos
        text = scan_token
        lexeme = @scanner.string.byteslice(start...@scanner.pos)
        token = Token.new(text, line, nil, text == '"' ? lexeme : nil)
        line += lexeme.count("\n")
        @tokens << token

        if GROUP_END.key?(text)
          groups << token
        elsif groups.any? && GROUP_END[groups.last.text] == text
          groups.pop.after_group = @tokens.length
        end
      end
    end
  end

  def [](index)
    @tokens[index]&.text
  end

  def line(index)
    @tokens[index].line
  end

  def after_group(index)
    @tokens[index]&.after_group
  end

  def string(index)
    literal = @tokens[index]&.literal
    return unless literal

    if literal.start_with?('"')
      literal[1...-1].gsub(STRING_ESCAPE) do
        unicode, ascii, continuation, character = Regexp.last_match.captures
        if unicode || ascii
          (unicode || ascii).delete("_").to_i(16).chr(Encoding::UTF_8)
        elsif continuation
          ""
        else
          ESCAPED_CHARACTERS.fetch(character, character)
        end
      end
    elsif (raw = literal.match(/\Ar(\#*)"(.*)"\1\z/m))
      raw[2]
    end
  end

  private

  def scan_token
    if @scanner.scan(/(?:b|c)?r(\#*)"/)
      terminator = '"' + @scanner[1]
      @scanner.terminate unless @scanner.scan_until(/#{Regexp.escape(terminator)}/)
      '"'
    elsif @scanner.scan(/(?:b|c)?"(?:\\.|[^"\\])*"/m)
      '"'
    elsif @scanner.scan(CHAR_LITERAL)
      "'"
    else
      @scanner.scan(IDENTIFIER) || @scanner.getch
    end
  end

  def skip_block_comment
    depth = 1
    newlines = 0
    while depth.positive? && (part = @scanner.scan_until(%r{/\*|\*/}))
      newlines += part.count("\n")
      depth += part.end_with?("/*") ? 1 : -1
    end
    @scanner.terminate if depth.positive?
    newlines
  end
end

class RustTestLint
  ITEM_KINDS = %w[fn struct enum union trait impl const type static macro_rules macro].freeze
  EXPRESSION_PREFIXES = %w[if while match in return break yield become].freeze
  ModuleScope = Struct.new(:name, :test_names, :directories)
  ModuleDeclaration = Struct.new(:name, :path, :directories)
  Scan = Struct.new(:diagnostics, :modules)

  def initialize(source, file)
    @tokens = RustTokens.new(source)
    @file = file
    @scan = Scan.new([], { false => [], true => [] })
  end

  def run
    lint_scope(0, ModuleScope.new(nil, {}, { false => [[]], true => [[]] }), false, module_body: true)
    @scan
  end

  private

  def lint_scope(index, scope, inside_test_module, module_body: false)
    inner_attributes, index = read_attributes(index, inner: true)
    if module_body && !inner_attributes.empty?
      inside_test_module ||= test_only?(inner_attributes)
      directories = scope.directories.select do |test, _|
        enabled?(inner_attributes, test)
      end
      scope = ModuleScope.new(scope.name, scope.test_names, directories)
    end

    while (text = @tokens[index]) && text != "}"
      attributes, item = read_attributes(index)
      keyword, visibility = item_keyword(item)
      kind = @tokens[keyword]
      body = keyword + 2

      if kind == "mod"
        name = @tokens[keyword + 1].delete_prefix("r#")
        locations = module_locations(attributes, scope)
        if @tokens[body] == "{"
          directories = locations.transform_values do |options|
            options.map { |parts, path| parts + [path || name] }
          end
          test_module = inside_test_module || test_only?(attributes)
          lint_scope(body + 1, ModuleScope.new(name, {}, directories), test_module, module_body: true)
          index = @tokens.after_group(body)
          next
        elsif @tokens[body] == ";"
          locations.each do |test, options|
            options.each { |parts, path| @scan.modules[test] << ModuleDeclaration.new(name, path, parts) }
          end
        end
      end

      unless attributes.empty?
        lint_placement(attributes, keyword, visibility) unless inside_test_module
        lint_name(attributes, keyword, scope) if kind == "fn"
      end

      # Skip macro token trees and attribute/argument groups. Ordinary Rust blocks can contain local items.
      if @tokens[item] == "{"
        lint_scope(item + 1, scope, inside_test_module)
      end
      index = after_macro(item, keyword) || @tokens.after_group(item) || item + 1
    end
  end

  def module_locations(attributes, scope)
    scope.directories.to_h do |test, directories|
      paths, required = path_options(attributes, test)
      paths << nil unless required
      [test, enabled?(attributes, test) ? directories.product(paths.uniq) : []]
    end
  end

  def path_options(attributes, test)
    paths = []
    required = false
    attributes.each do |index|
      case @tokens[index]&.delete_prefix("r#")
      when "path"
        path = @tokens.string(index + 2) if @tokens[index + 1] == "="
        if path
          paths << path
          required = true
        end
      when "cfg_attr"
        next unless @tokens[index + 1] == "("

        condition, *nested = arguments(index + 1)
        selected = cfg_predicate(condition, test)
        next if selected == false

        candidates, always = path_options(nested, test)
        paths.concat(candidates)
        required ||= selected == true && always
      end
    end
    [paths, required]
  end

  def after_macro(item, keyword)
    case @tokens[keyword]
    when "macro_rules"
      @tokens.after_group(keyword + 3) if @tokens[keyword + 1] == "!"
    when "macro"
      body = keyword + 2
      body = @tokens.after_group(body) if @tokens[body] == "("
      @tokens.after_group(body)
    else
      name = @tokens[item]
      if @tokens[item + 1] == "!" && name&.match?(RustTokens::IDENTIFIER) && !EXPRESSION_PREFIXES.include?(name)
        @tokens.after_group(item + 2)
      end
    end
  end

  def read_attributes(index, inner: false)
    attributes = []
    opening = index + (inner ? 2 : 1)
    while @tokens[index] == "#" && (!inner || @tokens[index + 1] == "!") && @tokens[opening] == "["
      attributes << opening + 1
      index = @tokens.after_group(opening)
      opening = index + (inner ? 2 : 1)
    end
    [attributes, index]
  end

  def item_keyword(index)
    visibility = @tokens[index] == "pub"
    if visibility
      index += 1
      index = @tokens.after_group(index) if @tokens[index] == "("
    end

    loop do
      case @tokens[index]
      when "unsafe", "async", "default", "safe"
        index += 1
      when "const"
        break unless %w[fn unsafe async extern].include?(@tokens[index + 1])

        index += 1
      when "extern"
        following = index + (@tokens[index + 1] == '"' ? 2 : 1)
        break unless @tokens[following] == "fn"

        index = following
      else
        break
      end
    end
    [index, visibility]
  end

  def lint_placement(attributes, keyword, visibility)
    return unless test_only?(attributes)

    kind = @tokens[keyword]
    if kind == "use" && !visibility
      report(keyword, "test-only use must be inside a cfg(test) module", placement: true)
    elsif kind == "extern" || (ITEM_KINDS.include?(kind) && @tokens[keyword + 1] != "{")
      report(keyword, "test-only item must be inside a cfg(test) module: #{kind}", placement: true)
    end
  end

  def test_only?(attributes)
    !enabled?(attributes, false) && enabled?(attributes, true)
  end

  def enabled?(attributes, test)
    attributes.none? { |attribute| cfg_attribute(attribute, test) == false }
  end

  # Evaluate with test fixed and every target/feature predicate unknown.
  def cfg_attribute(index, test)
    return true unless @tokens[index + 1] == "("

    case @tokens[index]&.delete_prefix("r#")
    when "cfg"
      cfg_predicate(index + 2, test)
    when "cfg_attr"
      condition, *attributes = arguments(index + 1)
      result = all(attributes.map { |attribute| cfg_attribute(attribute, test) })
      any([invert(cfg_predicate(condition, test)), result])
    else
      true
    end
  end

  def cfg_predicate(index, test)
    predicate = @tokens[index]&.delete_prefix("r#")
    case predicate
    when "test"
      @tokens[index + 1] == "=" ? nil : test
    when "true"
      true
    when "false"
      false
    when "all", "any", "not"
      return nil unless @tokens[index + 1] == "("

      values = arguments(index + 1).map { |argument| cfg_predicate(argument, test) }
      case predicate
      when "all" then all(values)
      when "any" then any(values)
      when "not" then invert(values.first)
      end
    end
  end

  def arguments(opening)
    arguments = []
    closing = @tokens.after_group(opening) - 1
    index = opening + 1
    while index < closing
      arguments << index
      while index < closing && @tokens[index] != ","
        index = @tokens.after_group(index) || index + 1
      end
      index += 1
    end
    arguments
  end

  def all(values)
    return false if values.include?(false)

    true unless values.include?(nil)
  end

  def any(values)
    return true if values.include?(true)

    false unless values.include?(nil)
  end

  def invert(value)
    !value unless value.nil?
  end

  def lint_name(attributes, keyword, scope)
    return unless attributes.any? { |index| test_attribute?(index) }

    name = @tokens[keyword + 1].delete_prefix("r#")
    if name.include?("returns_expected")
      report(keyword, "test name must not contain `returns_expected`: #{name} (use a concrete result, drop the suffix if the result is already implied, or split the rstest if cases do not share one guarantee)")
    end

    if (previous = scope.test_names[name])
      report(keyword, "duplicate test name in same module: #{name} (first seen at line #{previous})")
    else
      scope.test_names[name] = @tokens.line(keyword)
    end

    prefix = scope.name&.sub(/_tests?\z/, "")
    if prefix && prefix != "tests" && !prefix.empty? && name.start_with?("#{prefix}_")
      report(keyword, "redundant module prefix `#{prefix}_` in test name: #{name}")
    end
  end

  def test_attribute?(index)
    if @tokens[index] == "tokio" && @tokens[index + 1] == ":" && @tokens[index + 2] == ":"
      return @tokens[index + 3] == "test" && @tokens[index + 4] != ":"
    end

    %w[test rstest].include?(@tokens[index]) && @tokens[index + 1] != ":"
  end

  def report(index, message, placement: false)
    @scan.diagnostics << ["#{@file}:#{@tokens.line(index)} #{message}", placement]
  end
end

class RustModuleFiles
  def initialize(scans)
    @scans = scans
  end

  def test_only_files
    # Check possible references from crate roots and module files before walking each build.
    referenced = Set.new
    @scans.each_key do |file|
      [File.dirname(file), owned_directory(file)].uniq.each do |directory|
        [false, true].each do |test|
          children(file, directory, test).each { |child, _| referenced.add(child) }
        end
      end
    end

    roots = @scans.keys.select { |file| !referenced.include?(file) || cargo_root?(file) }
    pending = roots.flat_map { |file| [[file, File.dirname(file), false], [file, File.dirname(file), true]] }
    visited = Set.new
    production_files = Set.new
    test_files = Set.new
    until pending.empty?
      location = pending.pop
      next unless visited.add?(location)

      file, directory, test = location
      (test ? test_files : production_files).add(file)
      children(file, directory, test).each do |child, child_directory|
        pending << [child, child_directory, test]
      end
    end
    test_files - production_files
  end

  private

  def cargo_root?(file)
    directory = File.dirname(file)
    case File.basename(file)
    when "lib.rs", "main.rs", "build.rs"
      File.file?(File.join(directory, "Cargo.toml")) ||
        (File.basename(directory) == "src" && File.file?(File.join(directory, "..", "Cargo.toml")))
    else
      %w[tests examples benches].include?(File.basename(directory)) &&
        File.file?(File.join(directory, "..", "Cargo.toml"))
    end
  end

  def children(file, directory, test)
    @scans.fetch(file).modules.fetch(test).filter_map do |declaration|
      if declaration.path
        base = declaration.directories.empty? ? File.dirname(file) : directory
        base = inline_directory(base, declaration.directories)
        target = File.expand_path(declaration.path, base)
        [target, File.dirname(target)] if @scans.key?(target)
      else
        base = inline_directory(directory, declaration.directories)
        stem = File.join(base, declaration.name)
        target = ["#{stem}.rs", File.join(stem, "mod.rs")].find { |candidate| @scans.key?(candidate) }
        [target, stem] if target
      end
    end
  end

  def inline_directory(base, parts)
    parts.reduce(base) { |directory, part| File.expand_path(part, directory) }
  end

  def owned_directory(file)
    basename = File.basename(file, ".rs")
    basename == "mod" ? File.dirname(file) : File.join(File.dirname(file), basename)
  end
end

paths = ARGV.empty? ? DEFAULT_PATHS : ARGV
scans = rust_files(paths).to_h do |file|
  [file, RustTestLint.new(File.read(file), file.delete_prefix("#{ROOT}/")).run]
end
test_only_files = RustModuleFiles.new(scans).test_only_files
errors = scans.flat_map do |file, scan|
  scan.diagnostics.filter_map { |message, placement| message unless placement && test_only_files.include?(file) }
end

if errors.empty?
  puts "test-name lint passed"
else
  warn "test-name lint failed:"
  errors.each { |error| warn "- #{error}" }
  exit 1
end
