pipeline {
    agent any
    environment {
        V = 'v' + 1
        W = 2 + 'w'
        X = 'x' + true
        Y = 'y' + -3
        Z = "${V}" + 4.5
    }
    stages {
        stage('S') {
            steps { echo 'x' }
        }
    }
}
