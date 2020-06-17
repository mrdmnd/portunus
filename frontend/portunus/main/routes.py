import os

from flask import render_template, send_from_directory
from portunus.main import bp


@bp.route("/favicon.ico")
def favicon():
    return send_from_directory(
        os.path.join(bp.root_path, "static"), "icons/favicon.ico"
    )


@bp.route("/")
@bp.route("/index")
def index():
    return render_template("index.html")
