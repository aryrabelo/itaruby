# typed: true
# The declaration Sorbet needs for methods created by `define_method` in a
# loop. Tapioca generates this class of file from a booted app; itaruby
# infers the same surface from the class body with no file at all.
class Settings
  def host; end
  def port; end
  def scheme; end
  def host=(value); end
  def port=(value); end
  def scheme=(value); end
end
