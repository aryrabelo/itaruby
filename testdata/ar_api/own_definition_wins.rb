class ArApiOwnDefinitionWins < ActiveRecord::Base
  def initialize
  end

  def save
    true
  end
end

ArApiOwnDefinitionWins.new.save
