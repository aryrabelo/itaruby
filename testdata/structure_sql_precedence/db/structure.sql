--
-- Precedence fixture (bead ita-muf): declares `quantity` as `text` here,
-- conflicting on purpose with the sibling db/schema.rb's `integer`. This
-- file must never be read while db/schema.rb exists in the same project.
--

CREATE TABLE public.widgets (
    quantity text
);
