import os

basedir = os.path.abspath(os.path.dirname(__file__))


class Config(object):
    SECRET_KEY = os.environ.get("SECRET_KEY") or "e38425d0dcea98645bee3b0f3f3aaf5b"
    SQLALCHEMY_DATABASE_URI = os.environ.get(
        "SQLALCHEMY_DATABSE_URI"
    ) or "sqlite:///" + os.path.join(basedir, "portunus", "site.db")
    SQLALCHEMY_TRACK_MODIFICATIONS = False
    MAIL_SERVER = os.environ.get("MAIL_SERVER")
    MAIL_PORT = os.environ.get("MAIL_SERVER") or 25
    MAIL_USE_TLS = os.environ.get("MAIL_SERVER") is not None
    MAIL_USERNAME = os.environ.get("MAIL_SERVER")
    MAIL_PASSWORD = os.environ.get("MAIL_SERVER")
    ADMINS = ["admin@portunuspl.us"]
