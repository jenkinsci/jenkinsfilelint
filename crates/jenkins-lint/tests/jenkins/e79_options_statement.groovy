pipeline {
    agent any
    options {
        def x = 1
    }
    stages {
        stage('S') {
            steps { echo 'x' }
        }
    }
}
