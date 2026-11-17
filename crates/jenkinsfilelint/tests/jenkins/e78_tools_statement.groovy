pipeline {
    agent any
    tools {
        def x = 1
    }
    stages {
        stage('S') {
            steps { echo 'x' }
        }
    }
}
