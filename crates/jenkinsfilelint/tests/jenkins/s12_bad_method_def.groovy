def foo( {
}
pipeline {
    agent any
    stages {
        stage('Build') {
            steps {
                echo 'Hello'
            }
        }
    }
}
