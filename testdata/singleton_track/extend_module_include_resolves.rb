# frozen_string_literal: true

# Bead ita-xta, RESOLUTION side: `extend M` lifts the instance methods of
# M AND of M's own ancestry onto the class object. The rails shape is
# `class Person::Gender; extend ActiveModel::Translation; end`, where
# `Translation`'s whole body is `include Naming` and `Naming` owns
# `model_name`. Reading only M's own method map missed it.
module XtaNaming
  def xta_model_name
    "gender"
  end
end

module XtaTranslation
  include XtaNaming
end

class XtaGender
  extend XtaTranslation
end

raise "bad" unless XtaGender.xta_model_name == "gender"
