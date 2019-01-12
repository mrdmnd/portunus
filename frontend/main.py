from flask import Flask, request, render_template
DEBUG = True

app = Flask(__name__)
app.config.from_object(__name__)

@app.route('/')
def show_homepage():
    return render_template('index.html')

if __name__ == '__main__':
    app.run(debug=DEBUG)
