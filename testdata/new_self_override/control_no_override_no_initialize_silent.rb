# Control (ita-gjb fail-closed arm): a class with no own `self.new` AND
# no visible `initialize` anywhere stays silent — this bead must not
# disturb that pre-existing carve-out.
class NewSelfOvNoOverrideNoInit
end

NewSelfOvNoOverrideNoInit.new(1, 2)
