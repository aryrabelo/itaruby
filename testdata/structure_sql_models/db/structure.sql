--
-- PostgreSQL database dump
--
-- Toy fixture for bead ita-muf (invented tables/columns, no client data).

SET statement_timeout = 0;
SET client_encoding = 'UTF8';

CREATE TABLE public.widgets (
    id bigint NOT NULL,
    "label" character varying(255),
    quantity integer,
    price numeric(10,2),
    active boolean DEFAULT true,
    CONSTRAINT widgets_price_check CHECK ((price >= (0)::numeric)),
    PRIMARY KEY (id)
);

CREATE TABLE sprockets (
    id bigint NOT NULL,
    count integer
);

CREATE SEQUENCE public.widgets_id_seq
    START WITH 1
    INCREMENT BY 1;

ALTER TABLE ONLY public.widgets ALTER COLUMN id SET DEFAULT nextval('public.widgets_id_seq'::regclass);
