pipeline {
    agent any
    environment {
        A = ('a')
        B = ('a' + 'b')
        C = ('a') + "${A}"
        D = (credentials('id'))
    }
    stages {
        stage('S') {
            steps { echo 'x' }
        }
    }
}
